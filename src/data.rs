//! Installed games keep player data outside the application and working directory.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

pub fn validate_world_name(name: &str) -> Result<(), String> {
    // Use names that are safe on every release platform, including Windows devices.
    let upper = name.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    if name.is_empty()
        || name.len() > 64
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        || device
    {
        return Err(
            "--world must be 1–64 letters, digits, hyphens or underscores, and not a Windows device name".into()
        );
    }
    Ok(())
}

pub fn prepare(override_dir: Option<&Path>) -> io::Result<PathBuf> {
    let root = match override_dir {
        Some(path) => std::path::absolute(path)?,
        None => dirs::data_local_dir()
            .ok_or_else(|| io::Error::other("no per-user data directory; use --data-dir <dir>"))?
            .join("VoxelCraft"),
    };
    fs::create_dir_all(&root)?;
    // Explicit data folders are isolated, especially for automated captures/tests.
    if override_dir.is_none() {
        migrate_legacy(&std::env::current_dir()?.join("saves"), &root.join("saves"))?;
    }
    fs::create_dir_all(root.join("saves"))?;
    Ok(root)
}

fn migrate_legacy(source: &Path, destination: &Path) -> io::Result<()> {
    if destination.try_exists()? || !source.try_exists()? {
        return Ok(());
    }
    // Copy to a sibling first. An interrupted or failed import never leaves a
    // half-imported saves directory that would suppress the next attempt.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let staging = destination.with_file_name(format!("saves.importing-{}-{stamp}", std::process::id()));
    let result = copy_directory(source, &staging).and_then(|()| fs::rename(&staging, destination));
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map_err(|e| {
        io::Error::new(e.kind(), format!("could not import {}; original saves are untouched: {e}", source.display()))
    })
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(source)?.is_dir() {
        return Err(io::Error::other("legacy saves must be a directory, not a symbolic link"));
    }
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::other("legacy saves contain a symbolic link or special file; import them manually"));
        }
    }
    Ok(())
}

struct LogOutput(File);

impl Write for LogOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write_all(buf)?;
        // A Windows GUI build has no console, but CLI/development runs still do.
        let _ = io::stderr().write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub fn init_logging(root: &Path) {
    let current = root.join("voxelcraft.log");
    let previous = root.join("voxelcraft.previous.log");
    if current.exists() {
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&current, previous);
    }
    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,voxelcraft=info"));
    match File::create(&current) {
        Ok(file) => {
            logger.target(env_logger::Target::Pipe(Box::new(LogOutput(file))));
        }
        Err(e) => eprintln!("Could not open {}: {e}", current.display()),
    }
    logger.init();
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        previous_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "voxelcraft-data-test-{}-{stamp}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn world_names_cannot_escape_saves_or_use_windows_devices() {
        for name in ["", "..", "../world", "a/b", "a\\b", "/world", "C:world", "options.txt", "CON", "com1", "LPT9"] {
            assert!(validate_world_name(name).is_err(), "{name}");
        }
        for name in ["world", "creative-42", "test_world", "COM10"] {
            assert!(validate_world_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn legacy_import_copies_every_world_and_options_without_changing_originals() {
        let temp = TempDir::new();
        let legacy = temp.0.join("legacy");
        let target = temp.0.join("saves");
        fs::create_dir_all(legacy.join("world")).unwrap();
        fs::create_dir_all(legacy.join("creative")).unwrap();
        fs::write(legacy.join("world/level.txt"), "seed=42\n").unwrap();
        fs::write(legacy.join("creative/chunks.bin"), [1, 2, 3]).unwrap();
        fs::write(legacy.join("options.txt"), "volume=0.5\n").unwrap();
        migrate_legacy(&legacy, &target).unwrap();
        assert_eq!(
            fs::read(target.join("world/level.txt")).unwrap(),
            fs::read(legacy.join("world/level.txt")).unwrap()
        );
        assert_eq!(fs::read(target.join("creative/chunks.bin")).unwrap(), [1, 2, 3]);
        assert_eq!(fs::read_to_string(target.join("options.txt")).unwrap(), "volume=0.5\n");
        fs::write(target.join("world/level.txt"), "seed=99\n").unwrap();
        migrate_legacy(&legacy, &target).unwrap();
        assert_eq!(fs::read_to_string(target.join("world/level.txt")).unwrap(), "seed=99\n");
        assert_eq!(fs::read_to_string(legacy.join("world/level.txt")).unwrap(), "seed=42\n");
    }

    #[test]
    fn invalid_legacy_import_leaves_no_partial_destination() {
        let temp = TempDir::new();
        let legacy = temp.0.join("legacy");
        let target = temp.0.join("saves");
        fs::write(&legacy, "not a directory").unwrap();
        assert!(migrate_legacy(&legacy, &target).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_do_not_import_files_outside_the_legacy_folder() {
        let temp = TempDir::new();
        let legacy = temp.0.join("legacy");
        let target = temp.0.join("saves");
        fs::create_dir(&legacy).unwrap();
        fs::write(temp.0.join("outside"), "private").unwrap();
        std::os::unix::fs::symlink(temp.0.join("outside"), legacy.join("link")).unwrap();
        assert!(migrate_legacy(&legacy, &target).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read_to_string(temp.0.join("outside")).unwrap(), "private");
    }

    #[test]
    fn explicit_data_directory_has_an_absolute_save_location() {
        let temp = TempDir::new();
        let root = prepare(Some(&temp.0.join("isolated"))).unwrap();
        assert!(root.is_absolute());
        assert!(root.join("saves").is_dir());
        assert!(!root.join("saves/world").exists());
    }
}
