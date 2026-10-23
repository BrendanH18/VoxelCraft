//! Installing a verified package.
//!
//! - **Windows:** the game exits and runs the downloaded Inno Setup installer
//!   silently. The installer waits for the game to close, upgrades the
//!   installation in place (rolling itself back if it fails) and relaunches
//!   the game.
//! - **Mac:** the new app is copied out of the disk image beside the
//!   installed one and checked. After the game exits, a watcher (this
//!   binary, run with `--finish-update`) swaps the two apps atomically and
//!   starts the new one. The old app is deleted once the new version reports
//!   that it started; otherwise the watcher swaps them back and reopens the
//!   old version.
//!
//! Other builds (source builds, Linux, a Mac app run from the disk image)
//! can't replace themselves; the game points the player at the release page.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::version::Version;

/// The package this build installs, if it's an official build's platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Package {
    Windows,
    Mac,
}

impl Package {
    pub fn for_this_build() -> Option<Self> {
        if cfg!(all(windows, target_arch = "x86_64")) {
            Some(Package::Windows)
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Package::Mac)
        } else {
            None
        }
    }

    /// The release download's name, as the packaging scripts write it.
    pub fn asset_name(self, version: Version) -> String {
        match self {
            Package::Windows => format!("VoxelCraft-{version}-windows-x64-Setup.exe"),
            Package::Mac => format!("VoxelCraft-{version}-macos-apple-silicon.dmg"),
        }
    }
}

/// Where the running game is installed, when it can update itself.
#[derive(Clone, Debug)]
pub enum Install {
    /// An Inno Setup installation directory.
    Windows,
    /// The `.app` bundle, in a folder the player can write to.
    Mac { bundle: PathBuf },
}

/// How this copy can be updated, or why it can't update itself.
pub fn detect() -> Result<Install, String> {
    let manual = "This copy of VoxelCraft can't update itself";
    let exe = std::env::current_exe().map_err(|_| manual.to_string())?;
    match Package::for_this_build() {
        Some(Package::Windows) => {
            // The installer puts its uninstaller beside the game.
            let dir = exe.parent().ok_or(manual)?;
            if dir.join("unins000.exe").exists() {
                Ok(Install::Windows)
            } else {
                Err(format!("{manual}: it wasn't installed with the VoxelCraft installer"))
            }
        }
        Some(Package::Mac) => {
            let bundle = exe.ancestors().nth(3).ok_or(manual)?;
            let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap_or_default();
            if bundle.extension().is_none_or(|e| e != "app") || !plist.contains("io.github.brendanh18.voxelcraft") {
                return Err(format!("{manual}: it isn't the VoxelCraft app"));
            }
            let path = bundle.to_string_lossy();
            if path.starts_with("/Volumes/") || path.contains("/AppTranslocation/") {
                return Err(format!("{manual} while it runs from the disk image. Move it to Applications first"));
            }
            let parent = bundle.parent().ok_or(manual)?;
            let probe = parent.join(format!(".voxelcraft-write-test-{}", std::process::id()));
            if std::fs::write(&probe, b"").is_err() {
                return Err(format!("{manual}: you can't change files in {}", parent.display()));
            }
            let _ = std::fs::remove_file(probe);
            Ok(Install::Mac { bundle: bundle.to_path_buf() })
        }
        None => Err(format!("{manual}: updates are installed only on Windows and Apple Silicon Macs")),
    }
}

impl Install {
    /// The update locks' files, always taken in this order: the data
    /// folder's (`updates`, holding its downloads), and for a Mac app also
    /// one beside it (the staged and rollback app, whatever the data folder).
    pub fn lock_paths(&self, updates: &Path) -> Vec<PathBuf> {
        match self {
            Install::Mac { bundle } => {
                vec![updates.join("update.lock"), bundle.with_file_name(".VoxelCraft-update.lock")]
            }
            Install::Windows => vec![updates.join("update.lock")],
        }
    }
}

/// A verified package, ready to install once the game exits.
#[derive(Debug)]
pub enum Prepared {
    Windows { installer: PathBuf },
    Mac { staged: PathBuf, bundle: PathBuf },
}

/// Gets a verified download ready to install: on a Mac, copies the new app
/// out of the disk image beside the installed one and checks it.
pub fn prepare(install: &Install, package: &Path, version: Version, work: &Path) -> Result<Prepared, String> {
    match install {
        Install::Windows => Ok(Prepared::Windows { installer: package.to_path_buf() }),
        Install::Mac { bundle } => {
            let staged = staged_path(bundle);
            let _ = std::fs::remove_dir_all(&staged);
            let mount = work.join("mount");
            let _ = std::fs::create_dir_all(&mount);
            run(Command::new("/usr/bin/hdiutil")
                .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
                .arg(&mount)
                .arg(package))?;
            let copied = run(Command::new("/usr/bin/ditto").arg(mount.join("VoxelCraft.app")).arg(&staged));
            let _ = run(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount).arg("-force"));
            let checked = copied
                .and_then(|()| {
                    run(Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&staged))
                })
                .and_then(|()| {
                    let out = Command::new("/usr/bin/plutil")
                        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
                        .arg(staged.join("Contents/Info.plist"))
                        .output()
                        .map_err(|e| e.to_string())?;
                    let found = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if found == version.to_string() {
                        Ok(())
                    } else {
                        Err(format!("the downloaded app is version {found:?}, not {version}"))
                    }
                });
            if let Err(e) = checked {
                let _ = std::fs::remove_dir_all(&staged);
                return Err(e);
            }
            Ok(Prepared::Mac { staged, bundle: bundle.clone() })
        }
    }
}

/// The new app's place beside the installed one while updating.
fn staged_path(bundle: &Path) -> PathBuf {
    bundle.with_file_name(".VoxelCraft-update.app")
}

impl Prepared {
    /// Throws away a prepared update that won't be installed.
    pub fn discard(&self) {
        if let Prepared::Mac { staged, .. } = self {
            let _ = std::fs::remove_dir_all(staged);
        }
    }
}

/// Starts the installer or the Mac watcher, which reopens the game with
/// `restart_args`. The Mac watcher inherits `locks`, the held update locks.
/// The caller must exit promptly.
pub fn launch(
    prepared: &Prepared,
    updates: &Path,
    version: Version,
    restart_args: &[OsString],
    locks: &[std::fs::File],
) -> Result<(), String> {
    match prepared {
        Prepared::Windows { installer } => {
            let mut command = Command::new(installer);
            command
                .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/UPDATE=1"])
                .arg(format!("/WAITPID={}", std::process::id()))
                // The installer reopens the game with these (installer.iss RestartParams).
                .args(installer_restart_params(restart_args))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            command.spawn().map(drop).map_err(|e| format!("could not start the installer: {e}"))
        }
        #[cfg(target_os = "macos")]
        Prepared::Mac { staged, bundle } => {
            use std::os::fd::AsRawFd;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            // The watcher inherits the locked files, so the locks never lapse
            // between the game exiting and the watcher taking over.
            let fds: Vec<i32> = locks.iter().map(|l| l.as_raw_fd()).collect();
            if fds.is_empty() || !fds.iter().all(|&fd| set_inherited(fd, true)) {
                fds.iter().for_each(|&fd| _ = set_inherited(fd, false));
                return Err("could not hand the update lock to the installer".into());
            }
            let fd_list = fds.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
            let spawned = Command::new(exe)
                .arg("--finish-update")
                .arg(std::process::id().to_string())
                .arg(staged)
                .arg(bundle)
                .arg(updates)
                .arg(version.to_string())
                .arg(fd_list)
                .args(restart_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            fds.iter().for_each(|&fd| _ = set_inherited(fd, false));
            spawned.map(drop).map_err(|e| format!("could not start the update: {e}"))
        }
        #[cfg(not(target_os = "macos"))]
        Prepared::Mac { bundle, .. } => {
            let _ = (updates, version, locks);
            Err(format!("{} can only be updated on a Mac", bundle.display()))
        }
    }
}

/// `--data-dir <dir>` and `--update-source <dir>` as Inno Setup parameters
/// (`/DATADIR=<dir>`, `/UPDATESOURCE=<dir>`). A trailing separator gets a
/// `.` after it (`C:\` becomes `C:\.`), so the installer's quotes around
/// the value aren't escaped and a drive root stays a root.
fn installer_restart_params(restart_args: &[OsString]) -> Vec<OsString> {
    restart_args
        .chunks(2)
        .filter_map(|pair| {
            let [flag, value] = pair else { return None };
            let name = match flag.to_str()? {
                "--data-dir" => "DATADIR",
                "--update-source" => "UPDATESOURCE",
                _ => return None,
            };
            let mut value = value.to_string_lossy().into_owned();
            if value.ends_with(['\\', '/']) {
                value.push('.');
            }
            Some(format!("/{name}={value}").into())
        })
        .collect()
}

/// Opens a web page in the player's browser.
pub fn open_url(url: &str) {
    let result = if cfg!(target_os = "macos") {
        Command::new("/usr/bin/open").arg(url).spawn()
    } else if cfg!(windows) {
        Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        log::warn!("could not open {url}: {e}");
    }
}

fn run(command: &mut Command) -> Result<(), String> {
    let out = command.stdin(Stdio::null()).output().map_err(|e| format!("could not run {command:?}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let program = command.get_program().to_string_lossy().rsplit('/').next().unwrap_or_default().to_string();
        Err(format!("{program} failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// `--finish-update <pid> <staged> <bundle> <updates> <version> <lock fds>
/// [game arguments...]`: the Mac watcher, holding the update locks it
/// inherited (comma-separated descriptors). Returns the process exit code.
#[cfg(target_os = "macos")]
pub fn finish_mac(args: &[String]) -> i32 {
    use std::io::Write;
    use std::time::{Duration, Instant};

    let [pid, staged, bundle, updates, version, fds, game_args @ ..] = args else { return 2 };
    let fds: Option<Vec<i32>> = fds.split(',').map(|fd| fd.parse().ok()).collect();
    let (Ok(pid), Some(version), Some(fds)) = (pid.parse::<i32>(), Version::parse(version), fds) else {
        return 2;
    };
    // Own the inherited locks (released when this returns), and keep the
    // game we start from inheriting them.
    if !fds.iter().all(|&fd| set_inherited(fd, false)) {
        return 2;
    }
    use std::os::fd::FromRawFd;
    // SAFETY: the game passed us these descriptors, open and locked, and
    // nothing else in this process uses them.
    let locks: Vec<std::fs::File> = fds.into_iter().map(|fd| unsafe { std::fs::File::from_raw_fd(fd) }).collect();
    let (staged, bundle, updates) = (PathBuf::from(staged), PathBuf::from(bundle), PathBuf::from(updates));
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(updates.join("update.log")).ok();
    let mut note = |msg: &str| {
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "{msg}");
        }
    };
    let executable = |app: &Path| app.join("Contents/MacOS/voxelcraft");
    let game = |app: &Path| {
        let mut command = Command::new(executable(app));
        command.args(game_args).stdin(Stdio::null());
        command
    };
    let started = super::started_marker(&updates, version);
    let _ = std::fs::remove_file(&started);

    // Wait for the game to exit (it does so right after starting us).
    let deadline = Instant::now() + Duration::from_secs(60);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if alive(pid) {
        // Never swap the app under a running game; the staged copy waits for
        // the next update to replace it.
        note("the game didn't exit; leaving the installed app as it is");
        return 1;
    }
    if let Err(e) = swap(&staged, &bundle) {
        note(&format!("could not swap in the new app: {e}"));
        let _ = std::fs::remove_dir_all(&staged);
        let _ = game(&bundle).spawn();
        return 1;
    }
    // `bundle` is now the new version, `staged` the old one.
    note(&format!("installed {version}; starting it"));
    let mut child = match game(&bundle).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            note(&format!("could not start the new version: {e}"));
            None
        }
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    let ok = loop {
        if started.exists() {
            break true;
        }
        let exited = child.as_mut().is_none_or(|c| c.try_wait().ok().flatten().is_some());
        if exited || Instant::now() > deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if ok {
        note("the new version started; removing the old app");
        let _ = std::fs::remove_file(&started);
        let _ = std::fs::remove_dir_all(&staged);
        drop(locks);
        return 0;
    }
    note("the new version didn't start; restoring the previous app");
    if let Some(mut c) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
    match swap(&staged, &bundle) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&staged);
        }
        Err(e) => note(&format!("could not restore the previous app: {e}")),
    }
    let _ = game(&bundle).spawn();
    1
}

/// Exchanges two paths in one step (`renamex_np` with `RENAME_SWAP`), so
/// the install location always holds a complete app.
#[cfg(target_os = "macos")]
fn swap(a: &Path, b: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(from: *const std::ffi::c_char, to: *const std::ffi::c_char, flags: u32) -> i32;
    }
    const RENAME_SWAP: u32 = 0x2;
    let a = CString::new(a.as_os_str().as_bytes())?;
    let b = CString::new(b.as_os_str().as_bytes())?;
    // SAFETY: both arguments are valid NUL-terminated paths for the call.
    if unsafe { renamex_np(a.as_ptr(), b.as_ptr(), RENAME_SWAP) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Whether child processes inherit `fd` (clears or sets close-on-exec).
#[cfg(target_os = "macos")]
fn set_inherited(fd: i32, inherited: bool) -> bool {
    unsafe extern "C" {
        fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
    const F_SETFD: i32 = 2;
    const FD_CLOEXEC: i32 = 1;
    // SAFETY: F_SETFD only changes the descriptor's flags; an invalid one fails.
    unsafe { fcntl(fd, F_SETFD, if inherited { 0 } else { FD_CLOEXEC }) != -1 }
}

#[cfg(target_os = "macos")]
fn alive(pid: i32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { kill(pid, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_packaging_scripts() {
        let v = Version(0, 11, 0);
        assert_eq!(Package::Windows.asset_name(v), "VoxelCraft-0.11.0-windows-x64-Setup.exe");
        assert_eq!(Package::Mac.asset_name(v), "VoxelCraft-0.11.0-macos-apple-silicon.dmg");
        assert_eq!(
            staged_path(Path::new("/Applications/VoxelCraft.app")),
            Path::new("/Applications/.VoxelCraft-update.app")
        );
    }

    #[test]
    fn restart_args_become_installer_parameters() {
        let args: Vec<OsString> =
            ["--data-dir", r"C:\Users\a b\Data\", "--update-source", r"D:\", "--new", "x"].map(Into::into).into();
        assert_eq!(
            installer_restart_params(&args),
            [OsString::from(r"/DATADIR=C:\Users\a b\Data\."), r"/UPDATESOURCE=D:\.".into()]
        );
    }

    #[test]
    fn test_builds_cannot_replace_themselves() {
        // The test binary lives in target/, not in an installer's folder or an app.
        assert!(detect().is_err());
    }

    #[test]
    fn mac_installs_lock_beside_the_app() {
        let mac = Install::Mac { bundle: "/Applications/VoxelCraft.app".into() };
        let updates = Path::new("/data/updates");
        assert_eq!(
            mac.lock_paths(updates),
            [updates.join("update.lock"), PathBuf::from("/Applications/.VoxelCraft-update.lock")]
        );
        assert_eq!(Install::Windows.lock_paths(updates), [updates.join("update.lock")]);
    }

    /// The handoff to the watcher: a lock inherited by a child process stays
    /// held after the game drops (or exits with) its own handle.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_inherited_lock_outlives_the_game() {
        use std::os::fd::AsRawFd;
        let dir = std::env::temp_dir().join(format!("voxelcraft-handoff-{}", std::process::id()));
        let path = dir.join("update.lock");
        let lock = super::super::lock_file(&path).unwrap();
        let fd = lock.as_raw_fd();
        assert!(set_inherited(fd, true));
        let mut watcher = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        assert!(set_inherited(fd, false));
        drop(lock);
        assert!(super::super::lock_file(&path).is_err(), "the watcher still holds it");
        watcher.kill().unwrap();
        watcher.wait().unwrap();
        assert!(super::super::lock_file(&path).is_ok(), "released when the watcher ends");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn swaps_folders_atomically() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-swap-{}", std::process::id()));
        let (a, b) = (dir.join("a.app"), dir.join("b.app"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("v"), "new").unwrap();
        std::fs::write(b.join("v"), "old").unwrap();
        swap(&a, &b).unwrap();
        assert_eq!(std::fs::read_to_string(b.join("v")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(a.join("v")).unwrap(), "old");
        assert!(swap(&dir.join("missing"), &b).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
