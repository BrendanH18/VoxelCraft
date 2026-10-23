//! The title's world screens driven the way players drive them: keyboard,
//! mouse and the controller adapter (`Nav`), through the toolkit's stack,
//! against real save folders in a temporary directory.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use super::create::CreateWorld;
use super::edit::EditWorld;
use super::home::Home;
use super::prompts::BackupConfirm;
use super::select::SelectWorld;
use super::*;
use crate::app::gui::tests::Memory;
use crate::app::gui::{Key, Nav, Screen, Size, Stack, inside};
use crate::world::storage::{Storage, VERSION_KEY};

/// A title host without a window: real saves on disk, everything else recorded.
struct Host {
    root: PathBuf,
    saves: PathBuf,
    settings: Settings,
    opened: Vec<PathBuf>,
    copied: Option<String>,
    offer: Option<String>,
    multiplayer: Cell<u32>,
}

impl Host {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("voxelcraft-title-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let saves = root.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        Self {
            root,
            saves,
            settings: Settings::default(),
            opened: Vec::new(),
            copied: None,
            offer: None,
            multiplayer: Cell::new(0),
        }
    }

    /// A world folder with `level.txt` lines, last played `age` seconds ago.
    fn world(&self, dir: &str, lines: &str, age: u64) -> PathBuf {
        let path = self.saves.join(dir);
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("level.txt");
        std::fs::write(&file, format!("seed=42\n{lines}")).unwrap();
        let when = SystemTime::now() - Duration::from_secs(age);
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(when).unwrap();
        path
    }

    fn level(&self, dir: &str) -> crate::world::storage::LevelInfo {
        Storage::new(self.saves.join(dir)).load_level().unwrap()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl gui::Host for Host {
    type Out = Action;
    fn click(&mut self) {}
    fn in_world(&self) -> bool {
        false
    }
}

impl OptionsHost for Host {
    fn settings(&self) -> &Settings {
        &self.settings
    }
    fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }
    fn apply_settings(&mut self) {}
    fn save_settings(&self) {}
    fn apply_fullscreen(&mut self) {}
}

impl TitleHost for Host {
    fn saves_dir(&self) -> &Path {
        &self.saves
    }
    fn open_path(&mut self, path: &Path) {
        self.opened.push(path.to_path_buf());
    }
    fn copy_text(&mut self, text: &str) {
        self.copied = Some(text.to_string());
    }
    fn update_offer(&self) -> Option<String> {
        self.offer.clone()
    }
    fn update_notice(&self) -> Option<String> {
        None
    }
    fn multiplayer(&mut self) -> Step<Self> {
        self.multiplayer.set(self.multiplayer.get() + 1);
        Step::Stay
    }
    fn updates(&mut self) -> Step<Self> {
        Step::Stay
    }
}

const SIZE: Size = (640.0, 360.0);
/// From the smallest UI `Ui::fitted_scale` allows to a large window.
const SIZES: [Size; 5] = [(320.0, 180.0), (320.0, 240.0), (480.0, 270.0), (640.0, 360.0), (960.0, 540.0)];

fn over_title(screen: impl Screen<Host>) -> Stack<Host> {
    let mut stack = Stack::with_clipboard(Box::new(Memory::default()));
    stack.open(Home);
    stack.open(screen);
    stack
}

fn key(stack: &mut Stack<Host>, host: &mut Host, code: KeyCode) -> Option<Action> {
    stack.key(host, &Key::new(code), SIZE)
}

fn typed(stack: &mut Stack<Host>, host: &mut Host, text: &str) {
    stack.key(host, &Key::typed(text), SIZE);
}

fn erase(stack: &mut Stack<Host>, host: &mut Host) {
    for _ in 0..40 {
        key(stack, host, KeyCode::Backspace);
    }
}

/// The world a play action opens, and what it creates.
fn played(out: Option<Action>) -> Option<(String, Option<NewWorld>)> {
    match out {
        Some(Action::Play(dir, new)) => Some((dir, new)),
        _ => None,
    }
}

/// The top screen's widget rects that are buttons, with their labels.
fn buttons(stack: &Stack<Host>, host: &Host) -> Vec<(usize, bool)> {
    let (shape, _) = stack.peek(host, SIZE);
    shape.slots.iter().enumerate().filter(|(_, s)| !s.scrolls && !s.field).map(|(i, s)| (i, s.focusable)).collect()
}

fn click(stack: &mut Stack<Host>, host: &mut Host, index: usize) -> Option<Action> {
    let (shape, _) = stack.peek(host, SIZE);
    let [x, y, w, h] = shape.slots[index].rect;
    stack.mouse_moved(host, (x + w / 2.0, y + h / 2.0), SIZE);
    let out = stack.mouse_button(host, MouseButton::Left, true, SIZE);
    stack.mouse_button(host, MouseButton::Left, false, SIZE);
    out
}

/// Ticks until `done` or a few seconds pass (backups run on a thread).
fn tick_until(stack: &mut Stack<Host>, host: &mut Host, done: impl Fn(&Stack<Host>, &Host) -> bool) -> Option<Action> {
    for _ in 0..500 {
        if let Some(out) = stack.tick(host, SIZE) {
            return Some(out);
        }
        if done(stack, host) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out");
}

/// Rows of the world list currently in the widget list.
fn rows(stack: &Stack<Host>, host: &Host) -> usize {
    stack.peek(host, SIZE).0.slots.iter().filter(|s| s.scrolls).count()
}

/// Three worlds: newest first `base`, then `future` (saved by a newer
/// version), then `one_life` (hardcore, from before v0.11 recorded versions).
fn three_worlds(host: &Host) {
    host.world("base", "name=Survival Base\nmode=survival\ndifficulty=easy\nversion=0.10.0\n", 10);
    host.world("future", "name=From The Future\nmode=creative\nversion=99.0.0\n", 20);
    host.world("one_life", "name=One Life\nhardcore=true\ndimension=nether\n", 30);
}

#[test]
fn title_menu_opens_worlds_options_and_quits() {
    let mut host = Host::new("home");
    let mut stack = Stack::with_clipboard(Box::new(Memory::default()));
    stack.open(Home);
    // Escape does nothing on the title screen.
    assert!(key(&mut stack, &mut host, KeyCode::Escape).is_none());
    assert_eq!(stack.depth(), 1);
    // Singleplayer with no worlds goes straight to Create New World, as in Java.
    stack.nav(&mut host, Nav::Down, SIZE);
    assert!(stack.nav(&mut host, Nav::Activate, SIZE).is_none());
    assert_eq!(stack.depth(), 2);
    let (dir, new) = played(key(&mut stack, &mut host, KeyCode::Enter)).expect("Enter in the name field creates");
    let new = new.unwrap();
    assert_eq!(
        (dir.as_str(), new.name.as_str(), new.mode, new.hardcore),
        ("New_World", "New World", GameMode::Survival, false)
    );
    // Back out, and with a world Singleplayer opens Select World.
    key(&mut stack, &mut host, KeyCode::Escape);
    assert_eq!(stack.depth(), 1);
    host.world("w", "name=W\n", 1);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert_eq!(stack.depth(), 2);
    assert_eq!(rows(&stack, &host), 1);
    stack.nav(&mut host, Nav::Back, SIZE);
    // Multiplayer, Options... and Quit Game by keyboard.
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    key(&mut stack, &mut host, KeyCode::Enter);
    assert_eq!(host.multiplayer.get(), 1);
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    key(&mut stack, &mut host, KeyCode::Enter);
    assert_eq!(stack.depth(), 2, "Options opens from the title screen");
    key(&mut stack, &mut host, KeyCode::Escape);
    key(&mut stack, &mut host, KeyCode::ArrowRight);
    assert!(matches!(key(&mut stack, &mut host, KeyCode::Enter), Some(Action::Quit)));
    // The update offer shows only while there is one.
    assert_eq!(Screen::<Host>::widgets(&Home, &host, SIZE).len(), 4);
    host.offer = Some("Update to 0.12".into());
    assert_eq!(Screen::<Host>::widgets(&Home, &host, SIZE).len(), 5);
}

#[test]
fn search_filters_by_name_and_folder_and_buttons_need_a_selection() {
    let mut host = Host::new("search");
    three_worlds(&host);
    let mut stack = over_title(SelectWorld::new(&host));
    assert_eq!(rows(&stack, &host), 3);
    // Nothing selected: Play, Edit, Delete and Re-Create are disabled.
    let enabled: Vec<bool> = buttons(&stack, &host).into_iter().map(|(_, on)| on).collect();
    assert_eq!(enabled, [false, true, false, false, false, true]);
    // The search field has focus; it matches names and folders, any case.
    typed(&mut stack, &mut host, "FUTURE");
    assert_eq!(rows(&stack, &host), 1);
    erase(&mut stack, &mut host);
    typed(&mut stack, &mut host, "one_");
    assert_eq!(rows(&stack, &host), 1, "the folder name matches too");
    erase(&mut stack, &mut host);
    typed(&mut stack, &mut host, "zzz");
    assert_eq!(rows(&stack, &host), 0);
    erase(&mut stack, &mut host);
    assert_eq!(rows(&stack, &host), 3);
}

#[test]
fn keyboard_and_controller_select_rows_and_play() {
    let mut host = Host::new("keys");
    three_worlds(&host);
    let mut stack = over_title(SelectWorld::new(&host));
    // Down from the search field reaches the first row and selects it.
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    let enabled: Vec<bool> = buttons(&stack, &host).into_iter().map(|(_, on)| on).collect();
    assert_eq!(enabled, [true, true, true, true, true, true]);
    let (dir, new) = played(key(&mut stack, &mut host, KeyCode::Enter)).expect("Enter plays the row");
    assert_eq!((dir.as_str(), new.is_none()), ("base", true));
    // The controller: down twice to the third world, A plays it.
    let mut stack = stack_at(&host);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Down, SIZE);
    let (dir, _) = played(stack.nav(&mut host, Nav::Activate, SIZE)).unwrap();
    assert_eq!(dir, "one_life");
    // Or the Play Selected World button, reached from the list.
    let mut stack = stack_at(&host);
    stack.nav(&mut host, Nav::Down, SIZE);
    let play = buttons(&stack, &host)[0].0;
    let (dir, _) = played(click(&mut stack, &mut host, play)).unwrap();
    assert_eq!(dir, "base");
}

fn stack_at(host: &Host) -> Stack<Host> {
    over_title(SelectWorld::new(host))
}

#[test]
fn a_click_selects_and_a_double_click_plays() {
    let mut host = Host::new("mouse");
    three_worlds(&host);
    let mut stack = over_title(SelectWorld::new(&host));
    // Rows follow the search field in the widget list.
    assert!(click(&mut stack, &mut host, 3).is_none(), "one click selects");
    assert!(buttons(&stack, &host)[0].1, "Play Selected World is enabled");
    let (dir, _) = played(click(&mut stack, &mut host, 3)).expect("a second click plays");
    assert_eq!(dir, "one_life");
    // A click on another row in between doesn't count as a double click.
    let mut stack = stack_at(&host);
    click(&mut stack, &mut host, 1);
    click(&mut stack, &mut host, 3);
    assert!(click(&mut stack, &mut host, 1).is_none());
}

#[test]
fn unreadable_worlds_can_be_deleted_but_not_played_or_edited() {
    let mut host = Host::new("unreadable");
    let path = host.world("broken", "", 10);
    std::fs::write(path.join("level.txt"), "seed=junk\n").unwrap();
    let mut stack = stack_at(&host);
    assert!(click(&mut stack, &mut host, 1).is_none(), "one click selects");
    let enabled: Vec<bool> = buttons(&stack, &host).into_iter().map(|(_, on)| on).collect();
    // Play, Create, Edit, Delete, Re-Create, Back: Java's `isDisabled` rules.
    assert_eq!(enabled, [false, true, false, true, false, true]);
    // A double click can't get around the disabled buttons.
    assert!(played(click(&mut stack, &mut host, 1)).is_none());
    assert_eq!(stack.depth(), 3, "a notice says why, over Select World");
}

#[test]
fn delete_asks_first_then_removes_the_world() {
    let mut host = Host::new("delete");
    three_worlds(&host);
    let mut stack = over_title(SelectWorld::new(&host));
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    let delete = buttons(&stack, &host)[3].0;
    // Cancel keeps it.
    click(&mut stack, &mut host, delete);
    assert_eq!(stack.depth(), 3, "the question is asked");
    stack.nav(&mut host, Nav::Back, SIZE);
    assert!(host.saves.join("base").exists());
    // Delete removes it, and the list reloads.
    click(&mut stack, &mut host, delete);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert_eq!(stack.depth(), 2);
    assert!(!host.saves.join("base").exists());
    stack.tick(&mut host, SIZE);
    assert_eq!(rows(&stack, &host), 2);
    // A world open in another window isn't deleted: Delete is disabled.
    let held = Storage::new(host.saves.join("future")).lock().unwrap();
    let mut stack = stack_at(&host);
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    assert!(!buttons(&stack, &host)[3].1, "locked worlds can't be deleted");
    assert!(select::delete(&host, "future").unwrap_err().contains("open in another VoxelCraft window"));
    drop(held);
    // Deleting the last world opens Create New World, as Java does.
    std::fs::remove_dir_all(host.saves.join("future")).unwrap();
    let mut stack = stack_at(&host);
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    let delete = buttons(&stack, &host)[3].0;
    click(&mut stack, &mut host, delete);
    key(&mut stack, &mut host, KeyCode::Tab);
    key(&mut stack, &mut host, KeyCode::Enter);
    stack.tick(&mut host, SIZE);
    assert_eq!(stack.depth(), 2);
    assert!(played(key(&mut stack, &mut host, KeyCode::Enter)).unwrap().1.is_some(), "Create New World is open");
}

#[test]
fn rename_saves_to_level_txt_and_keeps_the_version() {
    let mut host = Host::new("rename");
    three_worlds(&host);
    let mut stack = over_title(SelectWorld::new(&host));
    typed(&mut stack, &mut host, "future");
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    let edit = buttons(&stack, &host)[2].0;
    click(&mut stack, &mut host, edit);
    assert_eq!(stack.depth(), 3);
    // The name field has focus; a blank name can't be saved.
    erase(&mut stack, &mut host);
    let save = buttons(&stack, &host).iter().rev().nth(1).unwrap().1;
    assert!(!save, "Save is disabled for a blank name");
    typed(&mut stack, &mut host, "Renamed World");
    key(&mut stack, &mut host, KeyCode::Enter);
    assert_eq!(stack.depth(), 2, "Enter saves and closes");
    let level = host.level("future");
    assert_eq!(level.props["name"], "Renamed World");
    assert_eq!(level.props[VERSION_KEY], "99.0.0", "renaming doesn't make it this version's world");
    // The list shows the new name after reloading.
    stack.tick(&mut host, SIZE);
    erase(&mut stack, &mut host);
    typed(&mut stack, &mut host, "renamed");
    assert_eq!(rows(&stack, &host), 1);
    // Cancel leaves the name alone.
    let world = worlds::summary(&host.saves, "base").unwrap();
    let mut stack = over_title(EditWorld::new(&world, Default::default()));
    typed(&mut stack, &mut host, "X");
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(host.level("base").props["name"], "Survival Base");
}

#[test]
fn edit_world_backs_up_opens_folders_and_copies_the_seed() {
    let mut host = Host::new("edit");
    three_worlds(&host);
    let world = worlds::summary(&host.saves, "base").unwrap();
    let mut stack = over_title(EditWorld::new(&world, Default::default()));
    let ids: Vec<usize> = buttons(&stack, &host).into_iter().map(|(i, _)| i).collect();
    let [folder, backup, backups, seed, ..] = ids[..] else { panic!("{ids:?}") };
    click(&mut stack, &mut host, folder);
    click(&mut stack, &mut host, backups);
    assert_eq!(host.opened, [host.saves.join("base"), host.root.join("backups")]);
    assert!(host.root.join("backups").is_dir(), "Open Backups Folder makes it first");
    click(&mut stack, &mut host, seed);
    assert_eq!(host.copied.as_deref(), Some("42"));
    // Make Backup: a waiting screen, then back to the list with what was made.
    click(&mut stack, &mut host, backup);
    assert_eq!(stack.depth(), 3);
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(stack.depth(), 3, "the backup can't be backed out of");
    tick_until(&mut stack, &mut host, |s, _| s.depth() != 3);
    assert_eq!(stack.depth(), 2, "Edit World closed; the result shows over the list");
    let made: Vec<_> = std::fs::read_dir(host.root.join("backups")).unwrap().flatten().collect();
    assert_eq!(made.len(), 1);
    assert!(made[0].path().join("level.txt").exists());
    // A failed backup says why, over Edit World.
    let held = Storage::new(host.saves.join("base")).lock().unwrap();
    let mut stack = over_title(EditWorld::new(&world, Default::default()));
    click(&mut stack, &mut host, backup);
    // The waiting screen has no buttons; the error has one.
    tick_until(&mut stack, &mut host, |s, h| !s.peek(h, SIZE).0.slots.is_empty());
    assert_eq!(stack.depth(), 3, "the error is over Edit World");
    drop(held);
}

#[test]
fn newer_worlds_warn_before_opening() {
    let mut host = Host::new("newer");
    three_worlds(&host);
    let open = |host: &mut Host| {
        let mut stack = stack_at(host);
        typed(&mut stack, host, "future");
        key(&mut stack, host, KeyCode::ArrowDown);
        assert!(key(&mut stack, host, KeyCode::Enter).is_none(), "a newer world doesn't open straight away");
        assert_eq!(stack.depth(), 3);
        stack
    };
    // Cancel goes back to the list.
    let mut stack = open(&mut host);
    key(&mut stack, &mut host, KeyCode::Escape);
    assert_eq!(stack.depth(), 2);
    // "I Know What I'm Doing!" opens it without a backup.
    let mut stack = open(&mut host);
    let skip = buttons(&stack, &host)[1].0;
    let (dir, _) = played(click(&mut stack, &mut host, skip)).unwrap();
    assert_eq!(dir, "future");
    assert!(!host.root.join("backups").exists());
    // "Create Backup and Load" backs up, then opens it.
    let mut stack = open(&mut host);
    let backup = buttons(&stack, &host)[0].0;
    assert!(click(&mut stack, &mut host, backup).is_none());
    let (dir, _) = played(tick_until(&mut stack, &mut host, |_, _| false)).unwrap();
    assert_eq!(dir, "future");
    assert_eq!(std::fs::read_dir(host.root.join("backups")).unwrap().count(), 1);
    // If the backup fails (here the backups folder can't be made), the
    // world isn't opened.
    std::fs::remove_dir_all(host.root.join("backups")).unwrap();
    std::fs::write(host.root.join("backups"), "not a folder").unwrap();
    let mut stack = open(&mut host);
    click(&mut stack, &mut host, backup);
    assert!(tick_until(&mut stack, &mut host, |s, _| s.depth() == 3).is_none());
    assert_eq!(stack.peek(&host, SIZE).0.slots.len(), 1, "a notice says why");
    // Worlds from this version or older, or from before versions were saved, open directly.
    for dir in ["base", "one_life"] {
        let world = worlds::summary(&host.saves, dir).unwrap();
        assert!(played(Stack::<Host>::default().run(select::open(&host, &world))).is_some(), "{dir}");
    }
    assert!(saved_by_newer(&host.saves, "future") && !saved_by_newer(&host.saves, "base"));
}

#[test]
fn recreate_fills_create_new_world_from_the_world() {
    let mut host = Host::new("recreate");
    three_worlds(&host);
    let mut stack = stack_at(&host);
    typed(&mut stack, &mut host, "one");
    key(&mut stack, &mut host, KeyCode::ArrowDown);
    let recreate = buttons(&stack, &host)[4].0;
    click(&mut stack, &mut host, recreate);
    let (dir, new) = played(key(&mut stack, &mut host, KeyCode::Enter)).unwrap();
    let new = new.unwrap();
    // "One_Life", or "One_Life-2" where folder names ignore case.
    assert!(dir.starts_with("One_Life"), "{dir}");
    assert_eq!(
        (new.name.as_str(), new.seed, new.mode, new.hardcore, new.difficulty),
        ("One Life", Some(42), GameMode::Survival, true, Difficulty::Hard)
    );
    let world = worlds::summary(&host.saves, "base").unwrap();
    let mut stack = over_title(CreateWorld::recreate(&world));
    let (_, new) = played(key(&mut stack, &mut host, KeyCode::Enter)).unwrap();
    let new = new.unwrap();
    assert_eq!((new.mode, new.hardcore, new.difficulty), (GameMode::Survival, false, Difficulty::Easy));
}

#[test]
fn create_new_world_cycles_modes_and_switches_tabs() {
    let mut host = Host::new("create");
    let mut stack = over_title(CreateWorld::new());
    // Game tab: Tab bar (2), name, mode, difficulty, then the footer.
    assert_eq!(stack.peek(&host, SIZE).0.slots.len(), 7);
    let (mode, difficulty, create) = (3, 4, 5);
    // Survival, Hardcore (difficulty locked to Hard), Creative, Survival.
    click(&mut stack, &mut host, mode);
    assert!(!stack.peek(&host, SIZE).0.slots[difficulty].focusable, "Hardcore locks the difficulty");
    click(&mut stack, &mut host, mode);
    click(&mut stack, &mut host, difficulty);
    let (_, new) = played(click(&mut stack, &mut host, create)).unwrap();
    let new = new.unwrap();
    assert_eq!((new.mode, new.hardcore, new.difficulty), (GameMode::Creative, false, Difficulty::Hard));
    // The controller: up to the tab bar, right to World; focusing a tab shows it.
    let mut stack = over_title(CreateWorld::new());
    stack.nav(&mut host, Nav::Up, SIZE);
    stack.nav(&mut host, Nav::Right, SIZE);
    let (shape, _) = stack.peek(&host, SIZE);
    assert_eq!(shape.slots.len(), 5, "World tab: the seed field and the footer");
    stack.nav(&mut host, Nav::Down, SIZE);
    typed(&mut stack, &mut host, "-7");
    let (_, new) = played(click(&mut stack, &mut host, 3)).unwrap();
    assert_eq!(new.unwrap().seed, Some(-7i64 as u64));
    // Cancel goes back.
    let mut stack = over_title(CreateWorld::new());
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(stack.depth(), 1);
    // A taken folder name gets a number.
    host.world("New_World", "", 1);
    let mut stack = over_title(CreateWorld::new());
    assert_eq!(played(key(&mut stack, &mut host, KeyCode::Enter)).unwrap().0, "New_World-2");
}

/// Every title screen at every window size: on screen, no overlaps, and
/// Tab reaches every enabled widget.
#[test]
fn screens_fit_every_window() {
    let mut host = Host::new("fits");
    three_worlds(&host);
    for i in 0..8 {
        host.world(&format!("extra{i}"), "name=A world name long enough to need cutting short in the list\n", 100 + i);
    }
    host.offer = Some("Update to 0.12".into());
    let world = worlds::summary(&host.saves, "future").unwrap();
    type Make<'a> = &'a dyn Fn(&Host) -> Box<dyn gui::Dyn<Host>>;
    let make: [(&str, Make); 6] = [
        ("title", &|_| Box::new(Home)),
        ("select", &|h| Box::new(SelectWorld::new(h))),
        ("edit", &|_| Box::new(EditWorld::new(&world, Default::default()))),
        ("create", &|_| Box::new(CreateWorld::new())),
        ("recreate", &|_| Box::new(CreateWorld::recreate(&world))),
        ("backup", &|_| Box::new(BackupConfirm::downgrade(world.clone(), crate::update::Version(99, 0, 0)))),
    ];
    for (name, make) in make {
        for size in SIZES {
            let mut stack = Stack::with_clipboard(Box::new(Memory::default()));
            stack.run(Step::Push(make(&host)));
            let (shape, _) = stack.peek(&host, size);
            for (i, slot) in shape.slots.iter().enumerate().filter(|(_, s)| s.shown) {
                let [x, y, w, h] = slot.rect;
                assert!(x >= 0.0 && x + w <= size.0 && y >= 0.0 && y + h <= size.1, "{name} widget {i} at {size:?}");
                for (j, other) in shape.slots.iter().enumerate().filter(|(j, s)| *j != i && s.shown) {
                    let [ox, oy, ow, oh] = other.rect;
                    let overlap = x < ox + ow && ox < x + w && y < oy + oh && oy < y + h;
                    assert!(!overlap, "{name} widgets {i} and {j} overlap at {size:?}");
                }
            }
            if let Some([_, top, _, h]) = shape.list {
                assert!(top >= 20.0 && top + h <= size.1, "{name} list at {size:?}");
            }
            let focusable = shape.slots.iter().filter(|s| s.focusable).count();
            let mut seen = Vec::new();
            for _ in 0..focusable {
                stack.key(&mut host, &Key::new(KeyCode::Tab), size);
                let (shape, focus) = stack.peek(&host, size);
                let i = focus.expect("focused");
                let [x, y, w, h] = shape.slots[i].rect;
                assert!(shape.slots[i].shown, "{name} widget {i} scrolled into view at {size:?}");
                assert!(inside(shape.slots[i].rect, (x + w / 2.0, y + h / 2.0)));
                seen.push(i);
            }
            seen.sort_unstable();
            seen.dedup();
            // Create New World's tabs swap content as focus passes them.
            if !name.contains("create") {
                assert_eq!(seen.len(), focusable, "{name}: Tab reaches every widget at {size:?}");
            }
        }
    }
}
