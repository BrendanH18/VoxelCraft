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
/// `restart_args`. The caller must exit promptly.
pub fn launch(prepared: &Prepared, updates: &Path, version: Version, restart_args: &[OsString]) -> Result<(), String> {
    match prepared {
        Prepared::Windows { installer } => {
            let mut command = Command::new(installer);
            command
                .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/UPDATE=1"])
                .arg(format!("/WAITPID={}", std::process::id()))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            command.spawn().map(drop).map_err(|e| format!("could not start the installer: {e}"))
        }
        Prepared::Mac { staged, bundle } => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            Command::new(exe)
                .arg("--finish-update")
                .arg(std::process::id().to_string())
                .arg(staged)
                .arg(bundle)
                .arg(updates)
                .arg(version.to_string())
                .args(restart_args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(drop)
                .map_err(|e| format!("could not start the update: {e}"))
        }
    }
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

/// `--finish-update <pid> <staged> <bundle> <updates> <version> [game
/// arguments...]`: the Mac watcher. Returns the process exit code.
#[cfg(target_os = "macos")]
pub fn finish_mac(args: &[String]) -> i32 {
    use std::io::Write;
    use std::time::{Duration, Instant};

    let [pid, staged, bundle, updates, version, game_args @ ..] = args else { return 2 };
    let (Ok(pid), Some(version)) = (pid.parse::<i32>(), Version::parse(version)) else { return 2 };
    let (staged, bundle, updates) = (PathBuf::from(staged), PathBuf::from(bundle), PathBuf::from(updates));
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(updates.join("update.log")).ok();
    let mut note = |msg: &str| {
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "{msg}");
        }
    };
    // One watcher at a time: a second one must not swap the apps back.
    let Ok(lock) = std::fs::File::create(updates.join("watcher.lock")) else { return 1 };
    if lock.try_lock().is_err() {
        note("another update is already finishing; leaving it alone");
        return 1;
    }
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
    fn test_builds_cannot_replace_themselves() {
        // The test binary lives in target/, not in an installer's folder or an app.
        assert!(detect().is_err());
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
