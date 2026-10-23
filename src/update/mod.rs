//! The in-game updater (docs/updates.md): the title screen checks GitHub
//! for the latest published release and, when the player agrees, downloads
//! it, checks its signed checksum, backs up the worlds and installs it.

pub mod backup;
pub mod fetch;
pub mod install;
pub mod release;
pub mod verify;
pub mod version;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, unbounded};
use install::Package;

pub use fetch::Source;
pub use release::Release;
pub use version::Version;

pub const RELEASES_PAGE: &str = "https://github.com/BrendanH18/VoxelCraft/releases";

/// Where the updater stands, as the title screen shows it.
pub enum State {
    /// Turned off, or a check found nothing new (or failed: that is logged,
    /// not shown).
    Idle,
    Checking(Receiver<Result<Release, String>>),
    Available(Release),
    Working(Job),
    /// Verified and backed up; installing means exiting the game.
    Ready(Release, install::Prepared),
    /// The installer or watcher is running; the game is exiting. Never
    /// launched twice.
    Installing(Release),
    Failed(Release, String),
}

/// A download and preparation running on a worker thread.
pub struct Job {
    pub release: Release,
    pub step: Step,
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Downloading { done: u64, total: u64 },
    Verifying,
    BackingUp,
    Preparing,
}

enum Msg {
    Step(Step),
    /// Ready to install, with the update locks.
    Done(install::Prepared, Vec<std::fs::File>),
    Failed(String),
}

pub struct Updater {
    pub state: State,
    /// Shown once on the title screen: the result of the last update.
    pub notice: Option<String>,
    /// Whether this copy can install updates, or why not.
    pub install: Result<install::Install, String>,
    /// The offer was put off until the next launch ("Later").
    pub dismissed: bool,
    /// Install an offered update without asking (`--update-now`, for tests).
    pub auto: bool,
    /// Arguments for the game after an update: the data folder and update
    /// source only, never one-shot options.
    pub restart_args: Vec<std::ffi::OsString>,
    /// The update lock, from a finished download until the game exits; the
    /// Mac watcher inherits it from there.
    held: Vec<std::fs::File>,
    source: Source,
    /// The data folder's `updates` and `backups` folders.
    updates: PathBuf,
    backups: PathBuf,
}

impl Updater {
    /// An updater for the data folder `root`. Checks in the background when
    /// `check` is set.
    pub fn new(root: &Path, source: Source, check: bool) -> Self {
        let mut updater = Self {
            state: State::Idle,
            notice: None,
            install: install::detect(),
            dismissed: false,
            auto: false,
            restart_args: Vec::new(),
            held: Vec::new(),
            source,
            updates: root.join("updates"),
            backups: root.join("backups"),
        };
        if check {
            updater.check();
        }
        updater
    }

    pub fn check(&mut self) {
        if !matches!(self.state, State::Idle) {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let source = self.source.clone();
        std::thread::spawn(move || {
            let _ = tx.send(source.latest().and_then(|json| Release::parse(&json)));
        });
        self.state = State::Checking(rx);
    }

    /// Picks up finished checks and download progress. `skip` is the
    /// version the player chose to skip.
    pub fn poll(&mut self, skip: Option<Version>) {
        match &mut self.state {
            State::Checking(rx) => match rx.try_recv() {
                Ok(Ok(release)) => {
                    let current = Version::current();
                    if release.version > current && Some(release.version) != skip {
                        log::info!("VoxelCraft {} is available", release.version);
                        self.state = State::Available(release);
                    } else {
                        log::info!("no update: latest release is {}", release.version);
                        self.state = State::Idle;
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("update check failed: {e}");
                    self.state = State::Idle;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => self.state = State::Idle,
            },
            State::Working(job) => {
                let mut finished = None;
                for msg in job.rx.try_iter() {
                    match msg {
                        Msg::Step(step) => job.step = step,
                        Msg::Done(prepared, lock) => finished = Some(Ok((prepared, lock))),
                        Msg::Failed(e) => finished = Some(Err(e)),
                    }
                }
                match finished {
                    Some(Ok((prepared, _lock))) if job.cancel.load(Ordering::Relaxed) => {
                        prepared.discard();
                        self.state = State::Available(job.release.clone());
                    }
                    Some(Ok((prepared, locks))) => {
                        self.held = locks;
                        self.state = State::Ready(job.release.clone(), prepared);
                    }
                    Some(Err(e)) if e == fetch::CANCELLED => self.state = State::Available(job.release.clone()),
                    Some(Err(e)) => {
                        log::error!("update to {} failed: {e}", job.release.version);
                        self.state = State::Failed(job.release.clone(), e);
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// The release on offer, unless the player put it off.
    pub fn offer(&self) -> Option<&Release> {
        match &self.state {
            State::Available(r) | State::Failed(r, _) if !self.dismissed => Some(r),
            State::Working(job) => Some(&job.release),
            State::Ready(r, _) | State::Installing(r) => Some(r),
            _ => None,
        }
    }

    /// Starts downloading and preparing the offered release, with the saves
    /// folder to back up.
    pub fn start(&mut self, saves: &Path) {
        let (State::Available(release) | State::Failed(release, _)) = &self.state else { return };
        let release = release.clone();
        let (Ok(install), Some(package)) = (self.install.clone(), Package::for_this_build()) else { return };
        let (tx, rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Worker {
            release: release.clone(),
            package,
            locks: install.lock_paths(&self.updates),
            install,
            source: self.source.clone(),
            work: self.updates.join(release.version.to_string()),
            backups: self.backups.clone(),
            saves: saves.to_path_buf(),
            cancel: cancel.clone(),
            keys: verify::release_keys(),
        };
        std::thread::spawn(move || {
            let msg = match job.run(&tx) {
                Ok((prepared, locks)) => Msg::Done(prepared, locks),
                Err(e) => Msg::Failed(e),
            };
            let _ = tx.send(msg);
        });
        let total = Package::size(&release);
        self.state = State::Working(Job { release, step: Step::Downloading { done: 0, total }, rx, cancel });
    }

    pub fn cancel(&mut self) {
        if let State::Working(job) = &self.state {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Starts installing a prepared update. On success the game must exit
    /// straight away; on failure the error is shown.
    pub fn launch(&mut self) -> bool {
        let State::Ready(release, prepared) = &self.state else { return false };
        let version = release.version;
        let pending = self.updates.join(PENDING);
        let result = std::fs::create_dir_all(&self.updates)
            .and_then(|()| std::fs::write(&pending, format!("{version}\n")))
            .map_err(|e| format!("could not record the update: {e}"))
            .and_then(|()| install::launch(prepared, &self.updates, version, &self.restart_args, &self.held));
        match result {
            Ok(()) => {
                log::info!("installing VoxelCraft {version}; exiting");
                self.state = State::Installing(release.clone());
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&pending);
                self.held.clear();
                log::error!("could not install {version}: {e}");
                self.state = State::Failed(release.clone(), e);
                false
            }
        }
    }

    /// Run once the window and renderer are up: reports how the last update
    /// went, tells the Mac watcher this version started, and clears the
    /// finished download.
    pub fn after_start(&mut self) {
        let pending = self.updates.join(PENDING);
        let Some(target) = std::fs::read_to_string(&pending).ok().and_then(|s| Version::parse(s.trim())) else {
            return;
        };
        let current = Version::current();
        // An older version opened while an update is still finishing (its
        // lock is held) leaves the record for the new version, which must
        // see it to report that it started.
        if current < target && lock_file(&self.updates.join("update.lock")).is_err() {
            return;
        }
        let _ = std::fs::remove_file(&pending);
        if current >= target {
            let _ = std::fs::write(started_marker(&self.updates, target), b"");
            let _ = std::fs::remove_dir_all(self.updates.join(target.to_string()));
            self.notice = Some(format!("Updated to VoxelCraft {}", current.short()));
            log::info!("updated to {current}");
        } else {
            self.notice = Some(format!(
                "The update to VoxelCraft {} didn't finish. You're still on {}.",
                target.short(),
                current.short()
            ));
            log::warn!("update to {target} did not complete; still {current}");
            let _ = std::fs::remove_dir_all(self.updates.join(target.to_string()));
            // `--update-now` reopens with the same arguments: don't retry forever.
            self.auto = false;
        }
    }
}

/// Written before installing: the version being installed.
const PENDING: &str = "pending.txt";

/// Takes an update lock, held by one update at a time: from downloading
/// until the game exits, then (inherited, so it never lapses) by the Mac
/// watcher until the new version starts. Another window's update would
/// otherwise delete this one's download or the Mac rollback copy.
fn lock_file(path: &Path) -> Result<std::fs::File, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("could not lock updates: {e}"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err("another VoxelCraft window is already updating. Close it, then try again".into())
        }
        Err(std::fs::TryLockError::Error(e)) => Err(format!("could not lock updates: {e}")),
    }
}

/// The file a new version writes once it has started, for the Mac watcher.
fn started_marker(updates: &Path, version: Version) -> PathBuf {
    updates.join(format!("started-{version}"))
}

impl Package {
    /// The download size of this build's package in `release`, if listed.
    fn size(release: &Release) -> u64 {
        Package::for_this_build().and_then(|p| release.asset(&p.asset_name(release.version))).map_or(0, |a| a.size)
    }
}

struct Worker {
    release: Release,
    package: Package,
    /// The update locks: the data folder's (its downloads), and on a Mac also
    /// the installation's (the staged and rollback app beside it).
    locks: Vec<PathBuf>,
    install: install::Install,
    source: Source,
    /// `updates/<version>`: the downloads.
    work: PathBuf,
    backups: PathBuf,
    saves: PathBuf,
    cancel: Arc<AtomicBool>,
    /// Keys allowed to sign the checksums: the release keys, except in tests.
    keys: Vec<[u8; 32]>,
}

impl Worker {
    /// Downloads, checks, backs up and prepares the update while holding the
    /// update locks, which come back with the prepared update.
    fn run(&self, tx: &crossbeam_channel::Sender<Msg>) -> Result<(install::Prepared, Vec<std::fs::File>), String> {
        let locks = self.locks.iter().map(|path| lock_file(path)).collect::<Result<Vec<_>, _>>()?;
        match self.prepare(tx) {
            Ok(prepared) => Ok((prepared, locks)),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&self.work);
                Err(e)
            }
        }
    }

    fn prepare(&self, tx: &crossbeam_channel::Sender<Msg>) -> Result<install::Prepared, String> {
        let version = self.release.version;
        let name = self.package.asset_name(version);
        let asset = self.release.asset(&name).ok_or_else(|| format!("the release has no {name}"))?;
        let sums = self.release.asset(release::SUMS).ok_or("the release has no checksum list")?;
        let signature = self.release.asset(release::SIGNATURE).ok_or("the release's checksums aren't signed")?;
        let _ = std::fs::remove_dir_all(&self.work);
        std::fs::create_dir_all(&self.work).map_err(|e| format!("could not create {}: {e}", self.work.display()))?;

        let fetch = |asset: &release::Asset, progress: &mut dyn FnMut(u64)| {
            let dest = self.work.join(&asset.name);
            self.source.download(&asset.url, &dest, &self.cancel, progress).map(|()| dest)
        };
        let sums = std::fs::read(fetch(sums, &mut |_| {})?).map_err(|e| e.to_string())?;
        let signature = std::fs::read_to_string(fetch(signature, &mut |_| {})?).map_err(|e| e.to_string())?;
        let total = asset.size;
        let file = fetch(asset, &mut |done| {
            let _ = tx.send(Msg::Step(Step::Downloading { done, total }));
        })?;

        let _ = tx.send(Msg::Step(Step::Verifying));
        verify::check_package(&file, &name, &sums, &signature, &self.keys)?;

        let _ = tx.send(Msg::Step(Step::BackingUp));
        let backup = backup::backup_saves(&self.saves, &self.backups, &version.to_string())?;
        log::info!("backed up worlds to {}", backup.display());

        let _ = tx.send(Msg::Step(Step::Preparing));
        if self.cancel.load(Ordering::Relaxed) {
            return Err(fetch::CANCELLED.into());
        }
        let prepared = install::prepare(&self.install, &file, version, &self.work)?;
        if self.cancel.load(Ordering::Relaxed) {
            prepared.discard();
            return Err(fetch::CANCELLED.into());
        }
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voxelcraft-updater-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn release_json(tag: &str) -> String {
        format!(r#"{{"tag_name":"{tag}","html_url":"https://example.com","body":"notes","assets":[]}}"#)
    }

    fn settle(updater: &mut Updater, skip: Option<Version>) {
        for _ in 0..500 {
            updater.poll(skip);
            if !matches!(updater.state, State::Checking(_)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("check never finished");
    }

    #[test]
    fn offers_only_newer_unskipped_releases() {
        let root = scratch("offer");
        let fixture = root.join("fixture");
        std::fs::create_dir_all(&fixture).unwrap();
        let Version(major, minor, _) = Version::current();
        let newer = Version(major, minor + 1, 0);
        for (tag, skip, offered) in [
            (format!("v{}", newer.short()), None, true),
            (format!("v{}", newer.short()), Some(newer), false),
            (env!("CARGO_PKG_VERSION").to_string(), None, false),
            ("v0.1".to_string(), None, false),
        ] {
            std::fs::write(fixture.join("latest.json"), release_json(&tag)).unwrap();
            let mut updater = Updater::new(&root, Source::Folder(fixture.clone()), true);
            settle(&mut updater, skip);
            assert_eq!(updater.offer().is_some(), offered, "{tag} skip {skip:?}");
            if offered {
                updater.dismissed = true;
                assert!(updater.offer().is_none(), "Later hides it");
            }
        }
        // Checking off: nothing happens.
        let updater = Updater::new(&root, Source::Folder(fixture.clone()), false);
        assert!(matches!(updater.state, State::Idle));
        // A failed check is quiet.
        std::fs::remove_file(fixture.join("latest.json")).unwrap();
        let mut updater = Updater::new(&root, Source::Folder(fixture), true);
        settle(&mut updater, None);
        assert!(matches!(updater.state, State::Idle) && updater.offer().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_the_last_update_once() {
        let root = scratch("after");
        let updates = root.join("updates");
        let current = Version::current();
        std::fs::create_dir_all(updates.join(current.to_string())).unwrap();
        std::fs::write(updates.join(PENDING), format!("{current}\n")).unwrap();
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        updater.after_start();
        assert_eq!(updater.notice, Some(format!("Updated to VoxelCraft {}", current.short())));
        assert!(started_marker(&updates, current).exists(), "the Mac watcher sees the start");
        assert!(!updates.join(current.to_string()).exists(), "download removed");
        assert!(!updates.join(PENDING).exists());

        // An update that didn't take leaves the old version running.
        let newer = Version(current.0, current.1 + 1, 0);
        std::fs::write(updates.join(PENDING), format!("{newer}\n")).unwrap();
        // While the update still holds its lock, an older window leaves the
        // record alone.
        let busy = lock_file(&updates.join("update.lock")).unwrap();
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        updater.after_start();
        assert_eq!(updater.notice, None);
        assert!(updates.join(PENDING).exists(), "kept for the version being installed");
        drop(busy);
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        updater.auto = true;
        updater.after_start();
        assert!(updater.notice.as_deref().unwrap().contains("didn't finish"));
        assert!(!updater.auto, "a failed update isn't retried automatically");
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        updater.after_start();
        assert_eq!(updater.notice, None, "reported once");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Redraws keep arriving while the game exits; only the first may start
    /// the installer (found in a real update: several Mac watchers raced).
    #[cfg(unix)]
    #[test]
    fn installs_only_once() {
        let root = scratch("once");
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        let release = Release::parse(&release_json("v99.0")).unwrap();
        // A stand-in installer that ignores its arguments.
        updater.state = State::Ready(release, install::Prepared::Windows { installer: "/usr/bin/true".into() });
        updater.restart_args = vec!["--data-dir".into(), root.as_os_str().to_owned()];
        assert!(updater.launch());
        assert!(matches!(updater.state, State::Installing(_)));
        assert!(!updater.launch(), "a second redraw starts nothing");
        assert!(updater.offer().is_some(), "the update screen stays up while exiting");
        assert_eq!(std::fs::read_to_string(root.join("updates").join(PENDING)).unwrap(), "99.0.0\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Cancel pressed while the package was being prepared: the finished
    /// result is thrown away, not installed.
    #[test]
    fn a_late_cancel_wins() {
        let root = scratch("late-cancel");
        let mut updater = Updater::new(&root, Source::Folder(root.clone()), false);
        let release = Release::parse(&release_json("v99.0")).unwrap();
        let (tx, rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        updater.state = State::Working(Job { release, step: Step::Preparing, rx, cancel: cancel.clone() });
        updater.cancel();
        assert!(cancel.load(Ordering::Relaxed));
        let lock = lock_file(&root.join("updates").join("update.lock")).unwrap();
        tx.send(Msg::Done(install::Prepared::Windows { installer: "/nonexistent".into() }, vec![lock])).unwrap();
        updater.poll(None);
        assert!(matches!(updater.state, State::Available(_)), "offered again, not ready to install");
        assert!(lock_file(&root.join("updates").join("update.lock")).is_ok(), "the lock was released");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A fixture release one minor version ahead, holding a fake package
    /// for this platform with its checksum list signed by `seed`'s key.
    fn signed_fixture(root: &Path, seed: u8) -> (Worker, [u8; 32]) {
        let fixture = root.join("fixture");
        std::fs::create_dir_all(&fixture).unwrap();
        let current = Version::current();
        let newer = Version(current.0, current.1 + 1, 0);
        let name = Package::Windows.asset_name(newer);
        std::fs::write(fixture.join(&name), b"package").unwrap();
        let hash = verify::hex(&verify::sha256_file(&fixture.join(&name)).unwrap());
        let sums = format!("{hash}  {name}\n");
        std::fs::write(fixture.join(release::SUMS), &sums).unwrap();
        let pair = ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::new([seed; 32]));
        std::fs::write(fixture.join(release::SIGNATURE), verify::hex(pair.sk.sign(&sums, None).as_ref())).unwrap();
        let asset = |n: &str| format!(r#"{{"name":"{n}","size":7,"browser_download_url":"{n}"}}"#);
        let json = format!(
            r#"{{"tag_name":"v{}","body":"","assets":[{},{},{}]}}"#,
            newer.short(),
            asset(&name),
            asset(release::SUMS),
            asset(release::SIGNATURE)
        );
        std::fs::write(fixture.join("latest.json"), &json).unwrap();
        let worker = Worker {
            release: Release::parse(&json).unwrap(),
            package: Package::Windows,
            locks: vec![root.join("updates").join("update.lock")],
            install: install::Install::Windows,
            source: Source::Folder(fixture),
            work: root.join("updates").join(newer.to_string()),
            backups: root.join("backups"),
            saves: root.join("saves"),
            cancel: Arc::new(AtomicBool::new(false)),
            keys: verify::release_keys(),
        };
        (worker, *pair.pk)
    }

    /// The whole path up to installing: download, verify, back up, prepare.
    #[test]
    fn downloads_verifies_and_backs_up_a_signed_release() {
        let root = scratch("signed");
        let (mut worker, key) = signed_fixture(&root, 7);
        worker.keys = vec![key];
        let world = root.join("saves").join("alpha");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.txt"), "name=alpha\n").unwrap();
        let (tx, rx) = unbounded();
        let (prepared, _lock) = worker.run(&tx).unwrap();
        let steps: Vec<Step> =
            rx.try_iter().filter_map(|m| if let Msg::Step(s) = m { Some(s) } else { None }).collect();
        assert!(steps.contains(&Step::Downloading { done: 7, total: 7 }));
        let order = |s: Step| steps.iter().position(|&x| x == s).unwrap();
        assert!(order(Step::Verifying) < order(Step::BackingUp) && order(Step::BackingUp) < order(Step::Preparing));
        let install::Prepared::Windows { installer } = prepared else { panic!("expected the Windows path") };
        assert_eq!(std::fs::read(installer).unwrap(), b"package");
        let backups: Vec<_> = std::fs::read_dir(root.join("backups")).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].path().join("alpha/level.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Two windows can't update at once: the second stops before touching
    /// the first one's download.
    #[test]
    fn one_update_at_a_time() {
        let root = scratch("one-at-a-time");
        let (worker, _) = signed_fixture(&root, 7);
        let held = lock_file(&worker.locks[0]).unwrap();
        std::fs::create_dir_all(&worker.work).unwrap();
        std::fs::write(worker.work.join("other window's download"), b"x").unwrap();
        let (tx, _rx) = unbounded();
        let err = worker.run(&tx).unwrap_err();
        assert!(err.contains("already updating"), "{err}");
        assert!(worker.work.join("other window's download").exists());
        drop(held);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A release signed by any key but the release key is refused before
    /// anything is backed up.
    #[test]
    fn rejects_a_release_not_signed_by_the_release_key() {
        let root = scratch("unsigned");
        let (worker, _) = signed_fixture(&root, 9);
        let (tx, rx) = unbounded();
        let err = worker.run(&tx).unwrap_err();
        assert!(err.contains("not signed by VoxelCraft's release key"), "{err}");
        assert!(rx.try_iter().any(|m| matches!(m, Msg::Step(Step::Downloading { done: 7, total: 7 }))));
        assert!(!root.join("backups").exists(), "no backup before verifying");
        std::fs::remove_dir_all(root).unwrap();
    }
}
