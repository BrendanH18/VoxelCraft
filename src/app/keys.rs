//! Key bindings: Java 26.3's `KeyMapping` table for the actions VoxelCraft
//! has, with Java's names, categories and default keys, plus VoxelCraft's
//! own keys in a category of their own. Every gameplay key and mouse button
//! goes through this table; only Escape stays fixed, as in Java.
//!
//! Bindings are saved in `options.txt` as `key_<name>=<key>` lines with
//! Java's names on both sides (`key_key.forward=key.keyboard.w`).

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// A key or mouse button an action can be bound to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Input {
    /// Java's `key.keyboard.unknown`, shown as "Not Bound".
    Unbound,
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Java's `KeyMapping.Category`, in its sort order, then VoxelCraft's.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum Category {
    Movement,
    Misc,
    Multiplayer,
    Gameplay,
    Inventory,
    Debug,
    VoxelCraft,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Movement => "Movement",
            Category::Misc => "Miscellaneous",
            Category::Multiplayer => "Multiplayer",
            Category::Gameplay => "Gameplay",
            Category::Inventory => "Inventory",
            Category::Debug => "Debug",
            Category::VoxelCraft => "VoxelCraft",
        }
    }
}

/// Everything a key can be bound to. The order is the table's and the save
/// file's; the Key Binds screen sorts by category and name as Java does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Action {
    Attack,
    Use,
    Forward,
    Left,
    Back,
    Right,
    Jump,
    Sneak,
    Sprint,
    Drop,
    Inventory,
    Chat,
    PlayerList,
    PickBlock,
    Command,
    ToggleGui,
    TogglePerspective,
    Fullscreen,
    Hotbar1,
    Hotbar2,
    Hotbar3,
    Hotbar4,
    Hotbar5,
    Hotbar6,
    Hotbar7,
    Hotbar8,
    Hotbar9,
    DebugOverlay,
    Fly,
    GameMode,
    Vsync,
    Mute,
    RenderDistanceDown,
    RenderDistanceUp,
    SwapHands,
}

pub(super) const COUNT: usize = Action::SwapHands as usize + 1;

/// One row of the table: save name, label, category and default.
struct Spec {
    name: &'static str,
    label: &'static str,
    category: Category,
    default: Input,
    /// Java's `order` within a category (only the debug overlay has one).
    order: i8,
}

const fn spec(name: &'static str, label: &'static str, category: Category, default: Input) -> Spec {
    Spec { name, label, category, default, order: 0 }
}

const fn key(code: KeyCode) -> Input {
    Input::Key(code)
}

use Category as C;

const SPECS: [Spec; COUNT] = [
    spec("key.attack", "Attack/Destroy", C::Gameplay, Input::Mouse(MouseButton::Left)),
    spec("key.use", "Use Item/Place Block", C::Gameplay, Input::Mouse(MouseButton::Right)),
    spec("key.forward", "Walk Forward", C::Movement, key(KeyCode::KeyW)),
    spec("key.left", "Strafe Left", C::Movement, key(KeyCode::KeyA)),
    spec("key.back", "Walk Backward", C::Movement, key(KeyCode::KeyS)),
    spec("key.right", "Strafe Right", C::Movement, key(KeyCode::KeyD)),
    spec("key.jump", "Jump", C::Movement, key(KeyCode::Space)),
    spec("key.sneak", "Sneak", C::Movement, key(KeyCode::ShiftLeft)),
    spec("key.sprint", "Sprint", C::Movement, key(KeyCode::ControlLeft)),
    spec("key.drop", "Drop Selected Item", C::Inventory, key(KeyCode::KeyQ)),
    spec("key.inventory", "Open/Close Inventory", C::Inventory, key(KeyCode::KeyE)),
    spec("key.chat", "Open Chat", C::Multiplayer, key(KeyCode::KeyT)),
    spec("key.playerlist", "List Players", C::Multiplayer, key(KeyCode::Tab)),
    spec("key.pickItem", "Pick Block", C::Gameplay, Input::Mouse(MouseButton::Middle)),
    spec("key.command", "Open Command", C::Multiplayer, key(KeyCode::Slash)),
    spec("key.toggleGui", "Toggle GUI", C::Misc, key(KeyCode::F1)),
    spec("key.togglePerspective", "Toggle Perspective", C::Misc, key(KeyCode::F5)),
    spec("key.fullscreen", "Toggle Fullscreen", C::Misc, key(KeyCode::F11)),
    spec("key.hotbar.1", "Hotbar Slot 1", C::Inventory, key(KeyCode::Digit1)),
    spec("key.hotbar.2", "Hotbar Slot 2", C::Inventory, key(KeyCode::Digit2)),
    spec("key.hotbar.3", "Hotbar Slot 3", C::Inventory, key(KeyCode::Digit3)),
    spec("key.hotbar.4", "Hotbar Slot 4", C::Inventory, key(KeyCode::Digit4)),
    spec("key.hotbar.5", "Hotbar Slot 5", C::Inventory, key(KeyCode::Digit5)),
    spec("key.hotbar.6", "Hotbar Slot 6", C::Inventory, key(KeyCode::Digit6)),
    spec("key.hotbar.7", "Hotbar Slot 7", C::Inventory, key(KeyCode::Digit7)),
    spec("key.hotbar.8", "Hotbar Slot 8", C::Inventory, key(KeyCode::Digit8)),
    spec("key.hotbar.9", "Hotbar Slot 9", C::Inventory, key(KeyCode::Digit9)),
    Spec { order: -2, ..spec("key.debug.overlay", "Toggle Overlay", C::Debug, key(KeyCode::F3)) },
    // Java's `F` swaps hands; flying keeps double-tap jump and takes `R`.
    spec("key.voxelcraft.fly", "Toggle Flying", C::VoxelCraft, key(KeyCode::KeyR)),
    spec("key.voxelcraft.gameMode", "Switch Survival/Creative", C::VoxelCraft, key(KeyCode::KeyG)),
    spec("key.voxelcraft.vsync", "Toggle VSync", C::VoxelCraft, key(KeyCode::KeyV)),
    spec("key.voxelcraft.mute", "Toggle Sound", C::VoxelCraft, key(KeyCode::KeyM)),
    spec("key.voxelcraft.renderDistanceDown", "Lower Render Distance", C::VoxelCraft, key(KeyCode::BracketLeft)),
    spec("key.voxelcraft.renderDistanceUp", "Raise Render Distance", C::VoxelCraft, key(KeyCode::BracketRight)),
    spec("key.swapOffhand", "Swap Item With Offhand", C::Inventory, key(KeyCode::KeyF)),
];

impl Action {
    pub const ALL: [Action; COUNT] = {
        use Action::*;
        [
            Attack,
            Use,
            Forward,
            Left,
            Back,
            Right,
            Jump,
            Sneak,
            Sprint,
            Drop,
            Inventory,
            Chat,
            PlayerList,
            PickBlock,
            Command,
            ToggleGui,
            TogglePerspective,
            Fullscreen,
            Hotbar1,
            Hotbar2,
            Hotbar3,
            Hotbar4,
            Hotbar5,
            Hotbar6,
            Hotbar7,
            Hotbar8,
            Hotbar9,
            DebugOverlay,
            Fly,
            GameMode,
            Vsync,
            Mute,
            RenderDistanceDown,
            RenderDistanceUp,
            SwapHands,
        ]
    };

    fn spec(self) -> &'static Spec {
        &SPECS[self as usize]
    }
    /// Java's save name (`key.forward`).
    pub fn name(self) -> &'static str {
        self.spec().name
    }
    pub fn label(self) -> &'static str {
        self.spec().label
    }
    pub fn category(self) -> Category {
        self.spec().category
    }
    pub fn default_input(self) -> Input {
        self.spec().default
    }
    /// The hotbar slot (0..9) a hotbar action selects.
    pub fn hotbar(self) -> Option<usize> {
        (self as usize).checked_sub(Action::Hotbar1 as usize).filter(|&i| i < 9)
    }

    /// Every action in Java's Key Binds order: by category, then `order`,
    /// then name.
    pub fn sorted() -> Vec<Action> {
        let mut all = Action::ALL.to_vec();
        all.sort_by(|a, b| {
            let (a, b) = (a.spec(), b.spec());
            (a.category, a.order, a.label).cmp(&(b.category, b.order, b.label))
        });
        all
    }
}

/// The key each action is bound to, indexed by [`Action`]. Looking a binding
/// up is an array index, so the per-tick input path doesn't allocate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Bindings([Input; COUNT]);

impl Default for Bindings {
    fn default() -> Self {
        Self(std::array::from_fn(|i| Action::ALL[i].default_input()))
    }
}

impl Bindings {
    pub fn get(&self, action: Action) -> Input {
        self.0[action as usize]
    }

    pub fn set(&mut self, action: Action, input: Input) {
        self.0[action as usize] = input;
    }

    pub fn is_default(&self, action: Action) -> bool {
        self.get(action) == action.default_input()
    }

    pub fn all_default(&self) -> bool {
        Action::ALL.iter().all(|&a| self.is_default(a))
    }

    pub fn reset(&mut self, action: Action) {
        self.set(action, action.default_input());
    }

    /// Whether `input` is bound to `action`.
    pub fn is(&self, action: Action, input: Input) -> bool {
        input != Input::Unbound && self.get(action) == input
    }

    /// The actions a press of `input` triggers (every one bound to it, as in Java).
    pub fn actions(&self, input: Input) -> impl Iterator<Item = Action> + '_ {
        Action::ALL.into_iter().filter(move |&a| self.is(a, input))
    }

    /// Java's `KeyBindsList` collision rule: other actions on the same key,
    /// unless both are still on their defaults.
    pub fn conflicts(&self, action: Action) -> impl Iterator<Item = Action> + '_ {
        let input = self.get(action);
        Action::ALL.into_iter().filter(move |&other| {
            other != action
                && input != Input::Unbound
                && self.get(other) == input
                && !(self.is_default(other) && self.is_default(action))
        })
    }

    /// Reads one `key_<name>` line; false if `key` names no action.
    pub fn parse_line(&mut self, key: &str, value: &str) -> bool {
        let Some(name) = key.strip_prefix("key_") else { return false };
        let Some(action) = Action::ALL.into_iter().find(|a| a.name() == name) else { return false };
        if let Some(input) = Input::parse(value) {
            self.set(action, input);
        }
        true
    }

    /// Options saved before the off-hand (v0.13) list flying on `F`, which
    /// is now Java's swap-hands key: move flying to its new default rather
    /// than leave one press doing both. A player who rebinds swap-hands
    /// afterwards keeps whatever they choose.
    pub fn migrate(&mut self, text: &str) {
        let swap_line = format!("key_{}=", Action::SwapHands.name());
        if !text.lines().any(|l| l.trim_start().starts_with(&swap_line))
            && self.get(Action::Fly) == Action::SwapHands.default_input()
        {
            self.reset(Action::Fly);
        }
    }

    /// One `key_<name>=<key>` line per action.
    pub fn serialize(&self, out: &mut String) {
        for action in Action::ALL {
            out.push_str(&format!("key_{}={}\n", action.name(), self.get(action).name()));
        }
    }
}

/// Java's `InputConstants` names and English display names for the keys a
/// player can bind.
const KEYS: &[(KeyCode, &str, &str)] = &[
    (KeyCode::KeyA, "a", "A"),
    (KeyCode::KeyB, "b", "B"),
    (KeyCode::KeyC, "c", "C"),
    (KeyCode::KeyD, "d", "D"),
    (KeyCode::KeyE, "e", "E"),
    (KeyCode::KeyF, "f", "F"),
    (KeyCode::KeyG, "g", "G"),
    (KeyCode::KeyH, "h", "H"),
    (KeyCode::KeyI, "i", "I"),
    (KeyCode::KeyJ, "j", "J"),
    (KeyCode::KeyK, "k", "K"),
    (KeyCode::KeyL, "l", "L"),
    (KeyCode::KeyM, "m", "M"),
    (KeyCode::KeyN, "n", "N"),
    (KeyCode::KeyO, "o", "O"),
    (KeyCode::KeyP, "p", "P"),
    (KeyCode::KeyQ, "q", "Q"),
    (KeyCode::KeyR, "r", "R"),
    (KeyCode::KeyS, "s", "S"),
    (KeyCode::KeyT, "t", "T"),
    (KeyCode::KeyU, "u", "U"),
    (KeyCode::KeyV, "v", "V"),
    (KeyCode::KeyW, "w", "W"),
    (KeyCode::KeyX, "x", "X"),
    (KeyCode::KeyY, "y", "Y"),
    (KeyCode::KeyZ, "z", "Z"),
    (KeyCode::Digit1, "1", "1"),
    (KeyCode::Digit2, "2", "2"),
    (KeyCode::Digit3, "3", "3"),
    (KeyCode::Digit4, "4", "4"),
    (KeyCode::Digit5, "5", "5"),
    (KeyCode::Digit6, "6", "6"),
    (KeyCode::Digit7, "7", "7"),
    (KeyCode::Digit8, "8", "8"),
    (KeyCode::Digit9, "9", "9"),
    (KeyCode::Digit0, "0", "0"),
    (KeyCode::Enter, "enter", "Enter"),
    (KeyCode::Escape, "escape", "Escape"),
    (KeyCode::Backspace, "backspace", "Backspace"),
    (KeyCode::Tab, "tab", "Tab"),
    (KeyCode::Space, "space", "Space"),
    (KeyCode::Minus, "minus", "-"),
    (KeyCode::Equal, "equal", "="),
    (KeyCode::BracketLeft, "left.bracket", "["),
    (KeyCode::BracketRight, "right.bracket", "]"),
    (KeyCode::Backslash, "backslash", "\\"),
    (KeyCode::Semicolon, "semicolon", ";"),
    (KeyCode::Quote, "apostrophe", "'"),
    (KeyCode::Backquote, "grave.accent", "`"),
    (KeyCode::Comma, "comma", ","),
    (KeyCode::Period, "period", "."),
    (KeyCode::Slash, "slash", "/"),
    (KeyCode::CapsLock, "caps.lock", "Caps Lock"),
    (KeyCode::F1, "f1", "F1"),
    (KeyCode::F2, "f2", "F2"),
    (KeyCode::F3, "f3", "F3"),
    (KeyCode::F4, "f4", "F4"),
    (KeyCode::F5, "f5", "F5"),
    (KeyCode::F6, "f6", "F6"),
    (KeyCode::F7, "f7", "F7"),
    (KeyCode::F8, "f8", "F8"),
    (KeyCode::F9, "f9", "F9"),
    (KeyCode::F10, "f10", "F10"),
    (KeyCode::F11, "f11", "F11"),
    (KeyCode::F12, "f12", "F12"),
    (KeyCode::F13, "f13", "F13"),
    (KeyCode::F14, "f14", "F14"),
    (KeyCode::F15, "f15", "F15"),
    (KeyCode::F16, "f16", "F16"),
    (KeyCode::F17, "f17", "F17"),
    (KeyCode::F18, "f18", "F18"),
    (KeyCode::F19, "f19", "F19"),
    (KeyCode::F20, "f20", "F20"),
    (KeyCode::PrintScreen, "print.screen", "Print Screen"),
    (KeyCode::ScrollLock, "scroll.lock", "Scroll Lock"),
    (KeyCode::Pause, "pause", "Pause"),
    (KeyCode::Insert, "insert", "Insert"),
    (KeyCode::Home, "home", "Home"),
    (KeyCode::PageUp, "page.up", "Page Up"),
    (KeyCode::Delete, "delete", "Delete"),
    (KeyCode::End, "end", "End"),
    (KeyCode::PageDown, "page.down", "Page Down"),
    (KeyCode::ArrowRight, "right", "Right Arrow"),
    (KeyCode::ArrowLeft, "left", "Left Arrow"),
    (KeyCode::ArrowDown, "down", "Down Arrow"),
    (KeyCode::ArrowUp, "up", "Up Arrow"),
    (KeyCode::NumLock, "num.lock", "Num Lock"),
    (KeyCode::NumpadDivide, "keypad.divide", "Keypad /"),
    (KeyCode::NumpadMultiply, "keypad.multiply", "Keypad *"),
    (KeyCode::NumpadSubtract, "keypad.subtract", "Keypad -"),
    (KeyCode::NumpadAdd, "keypad.add", "Keypad +"),
    (KeyCode::NumpadEnter, "keypad.enter", "Keypad Enter"),
    (KeyCode::Numpad1, "keypad.1", "Keypad 1"),
    (KeyCode::Numpad2, "keypad.2", "Keypad 2"),
    (KeyCode::Numpad3, "keypad.3", "Keypad 3"),
    (KeyCode::Numpad4, "keypad.4", "Keypad 4"),
    (KeyCode::Numpad5, "keypad.5", "Keypad 5"),
    (KeyCode::Numpad6, "keypad.6", "Keypad 6"),
    (KeyCode::Numpad7, "keypad.7", "Keypad 7"),
    (KeyCode::Numpad8, "keypad.8", "Keypad 8"),
    (KeyCode::Numpad9, "keypad.9", "Keypad 9"),
    (KeyCode::Numpad0, "keypad.0", "Keypad 0"),
    (KeyCode::NumpadDecimal, "keypad.period", "Keypad ."),
    (KeyCode::NumpadEqual, "keypad.equal", "Keypad ="),
    (KeyCode::IntlBackslash, "world.1", "World 1"),
    (KeyCode::ContextMenu, "menu", "Menu"),
    (KeyCode::ControlLeft, "left.control", "Left Control"),
    (KeyCode::ShiftLeft, "left.shift", "Left Shift"),
    (KeyCode::AltLeft, "left.alt", ALT[0]),
    (KeyCode::SuperLeft, "left.win", SUPER[0]),
    (KeyCode::ControlRight, "right.control", "Right Control"),
    (KeyCode::ShiftRight, "right.shift", "Right Shift"),
    (KeyCode::AltRight, "right.alt", ALT[1]),
    (KeyCode::SuperRight, "right.win", SUPER[1]),
];

/// Java's `InputQuirks` display overrides for Alt and the Windows key.
const ALT: [&str; 2] =
    if cfg!(target_os = "macos") { ["Left Option", "Right Option"] } else { ["Left Alt", "Right Alt"] };
const SUPER: [&str; 2] = if cfg!(target_os = "macos") {
    ["Left Command", "Right Command"]
} else if cfg!(target_os = "windows") {
    ["Left Windows", "Right Windows"]
} else {
    ["Left Meta", "Right Meta"]
};

/// Java's mouse button numbers: 1 left, 2 middle, 3 right, then the side buttons.
fn mouse_number(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
        MouseButton::Other(n) => n.saturating_add(1),
    }
}

fn mouse_button(n: u16) -> MouseButton {
    match n {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        4 => MouseButton::Back,
        5 => MouseButton::Forward,
        n => MouseButton::Other(n - 1),
    }
}

impl Input {
    /// Java's save name: `key.keyboard.w`, `key.mouse.left`, `key.mouse.4`.
    pub fn name(self) -> String {
        match self {
            Input::Unbound => "key.keyboard.unknown".into(),
            Input::Key(code) => match KEYS.iter().find(|k| k.0 == code) {
                Some((_, name, _)) => format!("key.keyboard.{name}"),
                None => "key.keyboard.unknown".into(),
            },
            Input::Mouse(MouseButton::Left) => "key.mouse.left".into(),
            Input::Mouse(MouseButton::Middle) => "key.mouse.middle".into(),
            Input::Mouse(MouseButton::Right) => "key.mouse.right".into(),
            Input::Mouse(b) => format!("key.mouse.{}", mouse_number(b)),
        }
    }

    /// A saved name; `None` if it isn't one.
    pub fn parse(name: &str) -> Option<Self> {
        if name == "key.keyboard.unknown" {
            return Some(Input::Unbound);
        }
        if let Some(key) = name.strip_prefix("key.keyboard.") {
            return KEYS.iter().find(|k| k.1 == key).map(|k| Input::Key(k.0));
        }
        let mouse = name.strip_prefix("key.mouse.")?;
        Some(Input::Mouse(match mouse {
            "left" => MouseButton::Left,
            "middle" => MouseButton::Middle,
            "right" => MouseButton::Right,
            n => mouse_button(n.parse::<u16>().ok().filter(|&n| n >= 1)?),
        }))
    }

    /// What the Key Binds screen shows: "W", "Left Shift", "Right Button".
    pub fn display(self) -> String {
        match self {
            Input::Unbound => "Not Bound".into(),
            Input::Key(code) => {
                KEYS.iter().find(|k| k.0 == code).map_or_else(|| format!("{code:?}"), |k| k.2.to_string())
            }
            Input::Mouse(MouseButton::Left) => "Left Button".into(),
            Input::Mouse(MouseButton::Middle) => "Middle Button".into(),
            Input::Mouse(MouseButton::Right) => "Right Button".into(),
            Input::Mouse(b) => format!("Button {}", mouse_number(b)),
        }
    }

    /// Whether a key can be bound (Java can't bind keys it has no name for).
    pub fn bindable(self) -> bool {
        match self {
            Input::Key(code) => KEYS.iter().any(|k| k.0 == code),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_complete_and_in_order() {
        for (i, a) in Action::ALL.into_iter().enumerate() {
            assert_eq!(a as usize, i);
        }
        let mut names: Vec<_> = Action::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COUNT, "save names are unique");
        assert_eq!(Action::Hotbar1.hotbar(), Some(0));
        assert_eq!(Action::Hotbar9.hotbar(), Some(8));
        assert_eq!(Action::DebugOverlay.hotbar(), None);
        assert_eq!(Action::Jump.hotbar(), None);
    }

    #[test]
    fn java_defaults_and_names() {
        let b = Bindings::default();
        assert_eq!(b.get(Action::Forward).name(), "key.keyboard.w");
        assert_eq!(b.get(Action::Sneak).name(), "key.keyboard.left.shift");
        assert_eq!(b.get(Action::Attack).name(), "key.mouse.left");
        assert_eq!(b.get(Action::Use).name(), "key.mouse.right");
        assert_eq!(b.get(Action::PickBlock).name(), "key.mouse.middle");
        assert_eq!(b.get(Action::Command).display(), "/");
        assert_eq!(b.get(Action::Sprint).display(), "Left Control");
        assert_eq!(Input::Unbound.display(), "Not Bound");
        assert_eq!(Input::Mouse(MouseButton::Back).display(), "Button 4");
        assert!(b.all_default());
        // No two defaults share a key.
        assert!(Action::ALL.iter().all(|&a| b.conflicts(a).next().is_none()));
    }

    #[test]
    fn every_key_name_round_trips() {
        for &(code, _, _) in KEYS {
            let input = Input::Key(code);
            assert_eq!(Input::parse(&input.name()), Some(input), "{code:?}");
        }
        for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back, MouseButton::Other(7)]
        {
            assert_eq!(Input::parse(&Input::Mouse(b).name()), Some(Input::Mouse(b)));
        }
        assert_eq!(Input::parse("key.keyboard.unknown"), Some(Input::Unbound));
        assert_eq!(Input::parse("key.keyboard.nonsense"), None);
        assert_eq!(Input::parse("key.mouse.0"), None);
        assert_eq!(Input::parse("junk"), None);
    }

    #[test]
    fn conflicts_follow_java() {
        let mut b = Bindings::default();
        b.set(Action::Drop, Input::Key(KeyCode::KeyW));
        assert_eq!(b.conflicts(Action::Drop).collect::<Vec<_>>(), vec![Action::Forward]);
        assert_eq!(b.conflicts(Action::Forward).collect::<Vec<_>>(), vec![Action::Drop]);
        assert_eq!(b.actions(Input::Key(KeyCode::KeyW)).collect::<Vec<_>>(), vec![Action::Forward, Action::Drop]);
        b.set(Action::Drop, Input::Unbound);
        b.set(Action::Jump, Input::Unbound);
        assert!(b.conflicts(Action::Drop).next().is_none(), "unbound keys never conflict");
        assert!(b.actions(Input::Unbound).next().is_none(), "nothing fires on an unbound key");
        b.reset(Action::Drop);
        assert!(b.is_default(Action::Drop) && !b.all_default());
    }

    #[test]
    fn the_key_binds_order_is_javas() {
        let sorted = Action::sorted();
        let movement: Vec<_> =
            sorted.iter().filter(|a| a.category() == Category::Movement).map(|a| a.label()).collect();
        assert_eq!(
            movement,
            ["Jump", "Sneak", "Sprint", "Strafe Left", "Strafe Right", "Walk Backward", "Walk Forward"]
        );
        assert_eq!(sorted[0].category(), Category::Movement);
        assert_eq!(sorted.last().unwrap().category(), Category::VoxelCraft);
        let gameplay: Vec<_> =
            sorted.iter().filter(|a| a.category() == Category::Gameplay).map(|a| a.label()).collect();
        assert_eq!(gameplay, ["Attack/Destroy", "Pick Block", "Use Item/Place Block"]);
    }

    #[test]
    fn options_from_before_the_off_hand_move_flying_off_f() {
        let old = "key_key.voxelcraft.fly=key.keyboard.f\n";
        let mut b = Bindings::default();
        b.set(Action::Fly, Action::SwapHands.default_input());
        b.migrate(old);
        assert_eq!(b.get(Action::Fly), Action::Fly.default_input());
        assert!(b.actions(Action::SwapHands.default_input()).eq([Action::SwapHands]));

        // A newer file that chose F for flying keeps it.
        let mut b = Bindings::default();
        b.set(Action::Fly, Action::SwapHands.default_input());
        b.set(Action::SwapHands, Input::Unbound);
        let mut text = String::new();
        b.serialize(&mut text);
        b.migrate(&text);
        assert_eq!(b.get(Action::Fly), Action::SwapHands.default_input());
    }

    #[test]
    fn saved_lines_round_trip() {
        let mut b = Bindings::default();
        b.set(Action::Jump, Input::Mouse(MouseButton::Forward));
        b.set(Action::Inventory, Input::Unbound);
        let mut text = String::new();
        b.serialize(&mut text);
        assert!(text.contains("key_key.jump=key.mouse.5\n"));
        assert!(text.contains("key_key.inventory=key.keyboard.unknown\n"));
        let mut read = Bindings::default();
        for line in text.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(read.parse_line(k, v));
        }
        assert_eq!(read, b);
        assert!(!read.parse_line("key_key.nothing", "key.keyboard.w"));
        assert!(!read.parse_line("fov", "70"));
        assert!(read.parse_line("key_key.jump", "garbage"), "a known key with a bad value keeps its binding");
        assert_eq!(read.get(Action::Jump), Input::Mouse(MouseButton::Forward));
    }
}
