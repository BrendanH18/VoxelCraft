//! Pre-update backups of every world and the options file. Newer versions
//! upgrade worlds and older ones can't open them again, so the updater
//! copies the saves folder before installing.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::world::storage::Storage;

/// Pre-update backups kept (owner, 2026-10-22).
pub const KEEP: usize = 3;
const MARKER: &str = "-before-";

/// Copies `saves` to a new folder in `backups` named for the time and
/// `version`, then keeps only the newest [`KEEP`] pre-update backups.
///
/// Each world's session lock is held for the whole copy, so no other
/// VoxelCraft window can save it halfway through; a world open elsewhere
/// stops the backup.
pub fn backup_saves(saves: &Path, backups: &Path, version: &str) -> Result<PathBuf, String> {
    let mut locks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(saves) {
        for entry in entries.flatten() {
            let storage = Storage::new(entry.path());
            if !entry.path().is_dir() || !storage.exists() {
                continue;
            }
            match storage.lock() {
                Ok(lock) => locks.push(lock),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(format!(
                        "\"{}\" is open in another VoxelCraft window. Close it, then try again.",
                        entry.file_name().to_string_lossy()
                    ));
                }
                Err(e) => return Err(format!("could not lock {}: {e}", entry.path().display())),
            }
        }
    }
    std::fs::create_dir_all(backups).map_err(|e| format!("could not create {}: {e}", backups.display()))?;
    let base = format!("{}{MARKER}{version}", timestamp(SystemTime::now()));
    // Another backup made in the same second gets the next number, so the
    // names keep the order the backups were made in.
    let taken = std::fs::read_dir(backups)
        .map_err(|e| format!("could not read {}: {e}", backups.display()))?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| backup_order(&n).filter(|(time, _)| *time == base).map(|(_, k)| k))
        .max();
    let dest = match taken {
        None => backups.join(&base),
        Some(k) => backups.join(format!("{base}-{}", k + 1)),
    };
    // Copy beside the destination first, so an interrupted copy never looks
    // like a finished backup.
    let partial = backups.join(format!(".{}.partial", dest.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&partial);
    let copied = match saves.try_exists() {
        Ok(true) => copy_tree(saves, &partial),
        Ok(false) => std::fs::create_dir_all(&partial),
        Err(e) => Err(e),
    }
    .and_then(|()| std::fs::rename(&partial, &dest));
    drop(locks);
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(format!("could not back up your worlds: {e}"));
    }
    prune(backups, KEEP);
    Ok(dest)
}

/// Where a folder made by [`backup_saves`] sorts: its name up to the
/// version, and its same-second number (1 for none). `None` for any other
/// folder, which is never pruned.
fn backup_order(name: &str) -> Option<(&str, u32)> {
    let (time, rest) = name.split_at_checked(19)?;
    let shape = time.bytes().enumerate().all(|(i, b)| match i {
        4 | 7 | 13 | 16 => b == b'-',
        10 => b == b'_',
        _ => b.is_ascii_digit(),
    });
    let rest = rest.strip_prefix(MARKER)?;
    let (version, n) = match rest.split_once('-') {
        // Numbers are written from 2, without leading zeros.
        Some((v, n)) => (v, n.parse::<u32>().ok().filter(|&k| k >= 2 && k.to_string() == n)?),
        None => (rest, 1),
    };
    // Exactly as `Version` writes it: no `v`, no leading zeros, three parts.
    let canonical = super::Version::parse(version).is_some_and(|v| v.to_string() == version);
    (shape && canonical).then(|| (&name[..19 + MARKER.len() + version.len()], n))
}

/// Copies a directory tree, leaving out session locks (which another
/// handle can't read on Windows while held). A symbolic link or special
/// file fails the copy: leaving it out would leave a world out.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            if entry.file_name() != "session.lock" {
                std::fs::copy(entry.path(), target)?;
            }
        } else {
            return Err(std::io::Error::other(format!(
                "{} is a link or special file; back it up by hand, then update from the release page",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

/// Removes all but the newest `keep` pre-update backups, counting only the
/// newest backup made before each version: retrying an update replaces its
/// earlier backup rather than pushing out older ones. Names start with a
/// sortable timestamp; other folders are never touched.
fn prune(backups: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(backups) else { return };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| backup_order(n).is_some())
        .collect();
    names.sort_by(|a, b| backup_order(a).cmp(&backup_order(b)));
    let mut versions = std::collections::HashSet::new();
    let mut kept = 0;
    for old in names.iter().rev() {
        let version = old.split(MARKER).nth(1).unwrap_or_default().split('-').next().unwrap_or_default();
        if versions.insert(version.to_string()) && kept < keep {
            kept += 1;
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(backups.join(old)) {
            log::warn!("could not remove old backup {old}: {e}");
        }
    }
}

/// `2026-10-22_16-05-09` in UTC.
fn timestamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's days-to-civil conversion.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voxelcraft-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn world(saves: &Path, name: &str) -> PathBuf {
        let dir = saves.join(name);
        std::fs::create_dir_all(dir.join("DIM-1")).unwrap();
        std::fs::write(dir.join("level.txt"), format!("name={name}\n")).unwrap();
        std::fs::write(dir.join("DIM-1").join("chunks.bin"), [1, 2, 3]).unwrap();
        dir
    }

    #[test]
    fn copies_worlds_and_options_without_locks() {
        let root = scratch("copy");
        let saves = root.join("saves");
        world(&saves, "alpha");
        std::fs::write(saves.join("options.txt"), "fov=90\n").unwrap();
        let backups = root.join("backups");
        let dest = backup_saves(&saves, &backups, "0.11.0").unwrap();
        assert!(dest.file_name().unwrap().to_string_lossy().ends_with("-before-0.11.0"));
        assert_eq!(std::fs::read(dest.join("alpha/DIM-1/chunks.bin")).unwrap(), [1, 2, 3]);
        assert_eq!(std::fs::read_to_string(dest.join("options.txt")).unwrap(), "fov=90\n");
        assert!(!dest.join("alpha/session.lock").exists());
        // The lock was released: the world opens again.
        assert!(Storage::new(saves.join("alpha")).lock().is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_a_world_open_elsewhere() {
        let root = scratch("busy");
        let saves = root.join("saves");
        world(&saves, "alpha");
        let held = Storage::new(saves.join("alpha")).lock().unwrap();
        let err = backup_saves(&saves, &root.join("backups"), "0.11.0").unwrap_err();
        assert!(err.contains("\"alpha\" is open in another VoxelCraft window"), "{err}");
        assert!(!root.join("backups").exists(), "nothing written");
        drop(held);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A folder that can't be listed fails the backup instead of leaving
    /// it out.
    #[cfg(unix)]
    #[test]
    fn unreadable_folders_fail_the_backup() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("unreadable");
        let saves = root.join("saves");
        let dim = world(&saves, "alpha").join("DIM-1");
        std::fs::set_permissions(&dim, std::fs::Permissions::from_mode(0o300)).unwrap();
        let result = backup_saves(&saves, &root.join("backups"), "0.11.0");
        std::fs::set_permissions(&dim, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.unwrap_err().starts_with("could not back up your worlds"));
        assert_eq!(std::fs::read_dir(root.join("backups")).unwrap().count(), 0, "no partial backup left");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A world reached through a symbolic link would be locked but not
    /// copied; the backup refuses instead.
    #[cfg(unix)]
    #[test]
    fn links_fail_the_backup() {
        let root = scratch("links");
        let saves = root.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let elsewhere = world(&root, "elsewhere");
        std::os::unix::fs::symlink(&elsewhere, saves.join("linked")).unwrap();
        let err = backup_saves(&saves, &root.join("backups"), "0.11.0").unwrap_err();
        assert!(err.contains("is a link or special file"), "{err}");
        assert_eq!(std::fs::read_dir(root.join("backups")).unwrap().count(), 0, "no partial backup left");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_the_newest_three() {
        let root = scratch("prune");
        let backups = root.join("backups");
        for name in [
            "2026-10-01_00-00-00-before-0.10.0",
            "2026-10-02_00-00-00-before-0.10.1",
            "2026-10-03_00-00-00-before-0.11.0",
            "2026-10-04_00-00-00-before-0.12.0",
            // Two retries of a failed update to 0.12.0.
            "2026-10-04_00-05-00-before-0.12.0",
            "2026-10-04_00-05-00-before-0.12.0-2",
            "my-own-copy",
            // Players' own folders that merely look similar are never touched.
            "2025-before-manual-copy",
            "2026-10-04_00-05-00-before-0.12.0-copy",
            "2026-10-04 00-05-00-before-0.12.0",
            ".2026-10-05_00-00-00-before-0.13.0.partial",
        ] {
            std::fs::create_dir_all(backups.join(name)).unwrap();
        }
        prune(&backups, KEEP);
        let mut left: Vec<String> =
            std::fs::read_dir(&backups).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(
            left,
            [
                ".2026-10-05_00-00-00-before-0.13.0.partial",
                "2025-before-manual-copy",
                "2026-10-02_00-00-00-before-0.10.1",
                "2026-10-03_00-00-00-before-0.11.0",
                "2026-10-04 00-05-00-before-0.12.0",
                "2026-10-04_00-05-00-before-0.12.0-2",
                "2026-10-04_00-05-00-before-0.12.0-copy",
                "my-own-copy",
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Three backups before the same version in one second: the newest is
    /// kept, not the second.
    #[test]
    fn same_second_backups_keep_the_newest() {
        let root = scratch("same-second");
        let saves = root.join("saves");
        world(&saves, "alpha");
        let backups = root.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let base = format!("{}{MARKER}0.12.0", timestamp(SystemTime::now()));
        std::fs::create_dir_all(backups.join(format!("{base}-2"))).unwrap();
        let dest = backup_saves(&saves, &backups, "0.12.0").unwrap();
        // Unless the second ticked over, the new one is numbered after -2.
        assert!(dest.exists(), "the backup just made survives pruning");
        assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1, "the older retry is replaced");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recognises_only_its_own_names() {
        assert_eq!(backup_order("2026-10-22_16-05-09-before-0.11.0"), Some(("2026-10-22_16-05-09-before-0.11.0", 1)));
        assert_eq!(backup_order("2026-10-22_16-05-09-before-0.11.0-3"), Some(("2026-10-22_16-05-09-before-0.11.0", 3)));
        for other in [
            "2025-before-manual-copy",
            "2026-10-22_16-05-09-before-0.11",
            "2026-10-22_16-05-09-before-0.11.0-copy",
            "2026-10-22_16-05-09-before-0.11.0-",
            "2026-10-22_16-05-09-before-v0.11.0",
            "2026-10-22_16-05-09-before-0.011.0",
            "2026-10-22_16-05-09-before-0.11.0-1",
            "2026-10-22_16-05-09-before-0.11.0-02",
            "2026-10-22 16-05-09-before-0.11.0",
            "x026-10-22_16-05-09-before-0.11.0",
            ".2026-10-22_16-05-09-before-0.11.0.partial",
            "my-own-copy",
        ] {
            assert_eq!(backup_order(other), None, "{other}");
        }
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01_00-00-00");
        // 2026-10-22 16:05:09 UTC.
        assert_eq!(timestamp(UNIX_EPOCH + std::time::Duration::from_secs(1_792_685_109)), "2026-10-22_16-05-09");
        // 2024-02-29 23:59:59 UTC, a leap day.
        assert_eq!(timestamp(UNIX_EPOCH + std::time::Duration::from_secs(1_709_251_199)), "2024-02-29_23-59-59");
    }
}
