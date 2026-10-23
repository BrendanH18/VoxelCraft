//! Saved worlds as the Select World list shows them (Java's `LevelSummary`):
//! read from each save's `level.txt`, with the version that last saved it.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::GameMode;
use crate::simulation::difficulty::Difficulty;
use crate::update::Version;
use crate::world::storage::{Storage, VERSION_KEY};

const NAME_MAX: usize = 32;

/// A saved world in the list.
#[derive(Clone, Debug)]
pub(in crate::app) struct Summary {
    /// Save folder name (Java's level id).
    pub dir: String,
    pub name: String,
    pub mode: GameMode,
    pub hardcore: bool,
    pub difficulty: Difficulty,
    pub seed: u64,
    /// Where the player was when the world was last saved.
    pub nether: bool,
    /// Made before v0.6: new Overworld terrain won't line up with it.
    pub legacy: bool,
    pub played: Option<SystemTime>,
    /// The version that last saved it, as written (`0.11.0`); `None` for
    /// worlds saved before v0.11 recorded it.
    pub version: Option<String>,
    /// Open in another VoxelCraft window.
    pub locked: bool,
    /// Why `level.txt` couldn't be read; the rest are then defaults.
    pub unreadable: Option<String>,
}

impl Summary {
    /// Saved by a newer VoxelCraft than this one (Java's downgrade).
    pub fn newer(&self) -> Option<Version> {
        newer_than_this(self.version.as_deref())
    }

    /// The version as players see it: `0.11`, or Java's "unknown".
    pub fn version_name(&self) -> String {
        match self.version.as_deref() {
            Some(v) => Version::parse(v).map_or_else(|| v.to_string(), Version::short),
            None => "unknown".into(),
        }
    }

    /// Java's search: the name or folder contains the filter, ignoring case.
    pub fn matches(&self, filter: &str) -> bool {
        let filter = filter.to_lowercase();
        self.name.to_lowercase().contains(&filter) || self.dir.to_lowercase().contains(&filter)
    }

    /// Java's game-mode name in the list ("Survival Mode", "Hardcore Mode").
    pub fn mode_name(&self) -> &'static str {
        if self.hardcore {
            return "Hardcore Mode";
        }
        match self.mode {
            GameMode::Survival => "Survival Mode",
            GameMode::Creative => "Creative Mode",
            GameMode::Adventure => "Adventure Mode",
            GameMode::Spectator => "Spectator Mode",
        }
    }

    /// The folder and when it was last played: `world (10/23/26, 2:05 PM)`.
    pub fn folder_and_date(&self) -> String {
        match self.played {
            Some(t) => format!("{} ({})", self.dir, short_date(t)),
            None => self.dir.clone(),
        }
    }
}

/// A version that's newer than this game, from `level.txt`'s version key.
pub(in crate::app) fn newer_than_this(saved: Option<&str>) -> Option<Version> {
    saved.and_then(Version::parse).filter(|v| *v > Version::current())
}

/// One save folder's summary, or `None` if it isn't a world.
pub(in crate::app) fn summary(saves_dir: &Path, dir: &str) -> Option<Summary> {
    crate::data::validate_world_name(dir).ok()?;
    let path = saves_dir.join(dir);
    let storage = Storage::new(&path);
    if !storage.exists() {
        return None;
    }
    let loaded = storage.load_level();
    let unreadable = loaded.as_ref().err().map(|e| e.to_string());
    let level = loaded.ok();
    let prop = |k: &str| level.as_ref().and_then(|l| l.props.get(k)).cloned();
    let played = std::fs::metadata(path.join("level.txt")).and_then(|m| m.modified()).ok();
    Some(Summary {
        name: prop("name").unwrap_or_else(|| dir.to_string()),
        mode: prop("mode").and_then(|m| GameMode::from_name(&m)).unwrap_or_default(),
        hardcore: prop("hardcore").is_some_and(|v| v == "true" || v == "1"),
        difficulty: prop("difficulty").and_then(|d| Difficulty::from_name(&d)).unwrap_or_default(),
        seed: level.as_ref().map_or(0, |l| l.seed),
        nether: prop("dimension").as_deref() == Some("nether"),
        // Pre-0.6 worlds save as terrain 1 once reopened, or lack it.
        legacy: prop("terrain").and_then(|v| v.parse::<u8>().ok()).is_none_or(|v| v < 2),
        played,
        version: prop(VERSION_KEY),
        locked: storage.locked_elsewhere(),
        unreadable,
        dir: dir.to_string(),
    })
}

/// Saved worlds, most recently played first.
pub(in crate::app) fn list(saves_dir: &Path) -> Vec<Summary> {
    let Ok(dir) = std::fs::read_dir(saves_dir) else { return Vec::new() };
    let mut worlds: Vec<Summary> =
        dir.flatten().filter_map(|e| summary(saves_dir, &e.file_name().into_string().ok()?)).collect();
    worlds.sort_by(|a, b| b.played.cmp(&a.played).then_with(|| a.dir.cmp(&b.dir)));
    worlds
}

/// Java's short date and time in US English (`10/23/26, 2:05 PM`), in the
/// computer's time zone where it can be found.
pub(in crate::app) fn short_date(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (offset, zone) = match local_offset() {
        Some(offset) => (offset, ""),
        None => (0, " UTC"),
    };
    let local = secs + offset;
    let (year, month, day) = crate::update::backup::civil(local.div_euclid(86_400));
    let minutes = local.rem_euclid(86_400) / 60;
    let (hour, minute) = (minutes / 60, minutes % 60);
    let (h12, half) = (if hour % 12 == 0 { 12 } else { hour % 12 }, if hour < 12 { "AM" } else { "PM" });
    format!("{month}/{day}/{:02}, {h12}:{minute:02} {half}{zone}", year.rem_euclid(100))
}

/// Seconds east of UTC, read once from `date +%z`. Rust's standard library
/// has no time zones, and a crate for one display line isn't worth it.
fn local_offset() -> Option<i64> {
    static OFFSET: OnceLock<Option<i64>> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        if cfg!(windows) {
            return None;
        }
        let out = std::process::Command::new("date").arg("+%z").output().ok()?;
        parse_offset(String::from_utf8_lossy(&out.stdout).trim())
    })
}

/// `+0930` or `-0500` in seconds.
fn parse_offset(text: &str) -> Option<i64> {
    let (sign, digits) = match text.split_at_checked(1)? {
        ("+", d) => (1, d),
        ("-", d) => (-1, d),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m) = (digits[..2].parse::<i64>().ok()?, digits[2..].parse::<i64>().ok()?);
    Some(sign * (h * 3600 + m * 60))
}

/// A save folder name for a world called `name`: letters and digits kept,
/// spaces turned into underscores, made unique with a number if `taken`.
pub(in crate::app) fn folder_for(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base: String = name
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => Some(c),
            ' ' => Some('_'),
            _ => None,
        })
        .take(NAME_MAX)
        .collect();
    if base.is_empty() || crate::data::validate_world_name(&base).is_err() {
        base = format!("world{base}");
    }
    let mut dir = base.clone();
    let mut n = 2;
    while taken(&dir) {
        dir = format!("{base}-{n}");
        n += 1;
    }
    dir
}

/// A seed from the create screen: a number as typed, any other text hashed
/// (like Minecraft), or `None` for a random one.
pub(in crate::app) fn parse_seed(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(n) = text.parse::<i64>() {
        return Some(n as u64);
    }
    // FNV-1a.
    Some(text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)))
}

/// A seed as Java shows it: a signed 64-bit number.
pub(in crate::app) fn seed_text(seed: u64) -> String {
    (seed as i64).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_safe_and_unique() {
        assert_eq!(folder_for("My World", |_| false), "My_World");
        assert_eq!(folder_for("Ünïcode!?", |_| false), "ncode");
        assert_eq!(folder_for("???", |_| false), "world");
        assert_eq!(folder_for("con", |_| false), "worldcon");
        let taken = ["New_World", "New_World-2"];
        assert_eq!(folder_for("New World", |d| taken.contains(&d)), "New_World-3");
        for name in ["a b c", "x".repeat(80).as_str(), "nul"] {
            assert!(crate::data::validate_world_name(&folder_for(name, |_| false)).is_ok(), "{name}");
        }
    }

    #[test]
    fn seeds_parse_numbers_and_hash_text() {
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed(" 42 "), Some(42));
        assert_eq!(parse_seed("-1"), Some(u64::MAX));
        assert_eq!(parse_seed("glacier"), parse_seed("glacier"));
        assert_ne!(parse_seed("glacier"), parse_seed("Glacier"));
        // Re-Create fills the seed field with this; it parses back the same.
        for seed in [0, 42, u64::MAX, 1 << 63, parse_seed("glacier").unwrap()] {
            assert_eq!(parse_seed(&seed_text(seed)), Some(seed));
        }
        assert_eq!(seed_text(u64::MAX), "-1");
    }

    #[test]
    fn worlds_list_newest_first_with_names_and_versions() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-title-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, props) in [
            ("old", "name=Old Times\nmode=creative\ndifficulty=easy\nterrain=2\nversion=0.11.0\n"),
            ("new", "hardcore=true\ndimension=nether\n"),
        ] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            std::fs::write(dir.join(folder).join("level.txt"), format!("seed={}\n{props}", -5i64 as u64)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::create_dir_all(dir.join("not-a-world")).unwrap();
        std::fs::create_dir_all(dir.join("broken")).unwrap();
        std::fs::write(dir.join("broken").join("level.txt"), "seed=-1\nname=Broken\n").unwrap();
        let broken = summary(&dir, "broken").unwrap();
        assert!(broken.unreadable.as_deref().is_some_and(|e| e.contains("bad seed")), "{broken:?}");
        assert_eq!(broken.name, "broken", "nothing from an unreadable file is trusted");
        std::fs::remove_dir_all(dir.join("broken")).unwrap();
        let worlds = list(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(worlds.len(), 2, "{worlds:?}");
        let (newest, oldest) = (&worlds[0], &worlds[1]);
        assert_eq!((oldest.dir.as_str(), oldest.name.as_str()), ("old", "Old Times"));
        assert_eq!((oldest.mode_name(), oldest.version_name().as_str()), ("Creative Mode", "0.11"));
        assert_eq!((oldest.difficulty, oldest.seed, oldest.legacy), (Difficulty::Easy, -5i64 as u64, false));
        assert_eq!((newest.dir.as_str(), newest.name.as_str(), newest.mode_name()), ("new", "new", "Hardcore Mode"));
        assert!(newest.nether && newest.legacy && !newest.locked);
        assert_eq!(newest.version_name(), "unknown", "saved before v0.11");
        assert!(newest.matches("NEW") && oldest.matches("times") && oldest.matches("ol") && !oldest.matches("x"));
    }

    #[test]
    fn only_newer_versions_warn() {
        let current = Version::current();
        assert_eq!(newer_than_this(None), None, "worlds from before v0.11 open without a warning");
        assert_eq!(newer_than_this(Some(&current.to_string())), None);
        assert_eq!(newer_than_this(Some("0.1.0")), None);
        assert_eq!(newer_than_this(Some("not a version")), None);
        let next = Version(current.0, current.1 + 1, 0);
        assert_eq!(newer_than_this(Some(&next.to_string())), Some(next));
        let fix = Version(current.0, current.1, current.2 + 1);
        assert_eq!(newer_than_this(Some(&fix.to_string())), Some(fix));
    }

    #[test]
    fn dates_use_java_short_style() {
        assert_eq!(parse_offset("+0930"), Some(34_200));
        assert_eq!(parse_offset("-0500"), Some(-18_000));
        assert_eq!(parse_offset("0500"), None);
        assert_eq!(parse_offset("+05"), None);
        // 2026-10-22 16:05:09 UTC, shown in whatever zone this machine is in.
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_792_685_109);
        let text = short_date(t);
        let shape = text.split(", ").collect::<Vec<_>>();
        assert_eq!(shape.len(), 2, "{text}");
        assert!(shape[0].ends_with("/26") && shape[1].contains(':'), "{text}");
        assert!(shape[1].contains("AM") || shape[1].contains("PM"), "{text}");
        if local_offset() == Some(0) {
            assert_eq!(text, "10/22/26, 4:05 PM");
        }
    }
}
