//! The toolkit driven the way players drive it: mouse, keyboard and the
//! controller adapter (`Nav`), against a test screen.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use super::*;

/// A clipboard that stays in the test.
#[derive(Default)]
pub(in crate::app) struct Memory(pub Option<String>);

impl Clipboard for Memory {
    fn get(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn set(&mut self, text: &str) {
        self.0 = Some(text.to_string());
    }
}

#[derive(Default)]
pub(in crate::app) struct TestHost {
    pub clicks: u32,
    pub log: Vec<String>,
    pub volume: f32,
}

impl Host for TestHost {
    type Out = &'static str;
    fn click(&mut self) {
        self.clicks += 1;
    }
    fn in_world(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Id {
    A,
    B,
    Volume,
    Name,
    Off,
    Row(usize),
    Quit,
}

struct Test {
    name: TextField,
    rows: usize,
}

const SIZE: Size = (400.0, 300.0);

impl Test {
    fn new(rows: usize) -> Self {
        Self { name: TextField::new(10), rows }
    }
}

impl Screen<TestHost> for Test {
    type Id = Id;

    fn title(&self, _: &TestHost) -> String {
        "Test".into()
    }

    fn widgets(&self, host: &TestHost, _: Size) -> Vec<Widget<Id>> {
        let mut w = vec![
            Widget::button(Id::A, [10.0, 10.0, 100.0, 20.0], "A"),
            Widget::button(Id::B, [120.0, 10.0, 100.0, 20.0], "B"),
            Widget::slider(Id::Volume, [10.0, 40.0, 100.0, 20.0], "Volume", host.volume, 10),
            Widget::field(Id::Name, [120.0, 40.0, 100.0, 20.0]),
            Widget::button(Id::Off, [230.0, 40.0, 100.0, 20.0], "Off").enabled(false).tooltip("Never"),
        ];
        for i in 0..self.rows {
            w.push(Widget::button(Id::Row(i), [10.0, 100.0 + i as f32 * 25.0, 100.0, 20.0], "Row").scrolling());
        }
        w.push(Widget::button(Id::Quit, [10.0, 270.0, 100.0, 20.0], "Quit"));
        w
    }

    fn list(&self, _: &TestHost, _: Size) -> Option<Rect> {
        Some([0.0, 100.0, 400.0, 150.0])
    }

    fn activate(&mut self, host: &mut TestHost, id: Id) -> Step<TestHost> {
        host.log.push(format!("{id:?}"));
        match id {
            Id::B => Step::push(Test::new(0)),
            Id::Quit => Step::Out("quit"),
            Id::Row(9) => Step::Close,
            _ => Step::Stay,
        }
    }

    fn slide(&mut self, host: &mut TestHost, _: Id, t: f32) {
        host.volume = (t * 10.0).round() / 10.0;
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        (id == Id::Name).then_some(&mut self.name)
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        (id == Id::Name).then_some(&self.name)
    }
}

fn stack(rows: usize) -> (Stack<TestHost>, TestHost) {
    let mut stack = Stack::with_clipboard(Box::new(Memory::default()));
    stack.open(Test::new(rows));
    (stack, TestHost::default())
}

fn key(code: KeyCode) -> Key {
    Key::new(code)
}

fn focused(stack: &Stack<TestHost>) -> Option<usize> {
    stack.entries.last().and_then(|e| e.focus)
}

fn field_text(stack: &mut Stack<TestHost>, host: &TestHost) -> String {
    let entry = stack.entries.last_mut().unwrap();
    entry.screen.field_at(host, SIZE, 3).unwrap().text().to_string()
}

#[test]
fn tab_cycles_focus_skipping_disabled_widgets_and_wraps() {
    let (mut stack, mut host) = stack(0);
    let mut order = Vec::new();
    for _ in 0..6 {
        stack.key(&mut host, &key(KeyCode::Tab), SIZE);
        order.push(focused(&stack).unwrap());
    }
    // A, B, Volume, Name, Quit (Off is disabled), then back to A.
    assert_eq!(order, [0, 1, 2, 3, 5, 0]);
    let back = Key { shift: true, ..key(KeyCode::Tab) };
    stack.key(&mut host, &back, SIZE);
    assert_eq!(focused(&stack), Some(5));
}

#[test]
fn arrows_and_the_d_pad_move_to_the_nearest_widget_that_way() {
    let (mut stack, mut host) = stack(0);
    stack.nav(&mut host, Nav::Down, SIZE);
    assert_eq!(focused(&stack), Some(0), "the first press focuses the first widget");
    stack.nav(&mut host, Nav::Right, SIZE);
    assert_eq!(focused(&stack), Some(1));
    stack.key(&mut host, &key(KeyCode::ArrowDown), SIZE);
    assert_eq!(focused(&stack), Some(3), "straight down, to the field under B");
    stack.nav(&mut host, Nav::Right, SIZE);
    assert_eq!(focused(&stack), Some(3), "disabled widgets are skipped; nothing else to the right");
    stack.nav(&mut host, Nav::Up, SIZE);
    assert_eq!(focused(&stack), Some(1));
}

#[test]
fn enter_space_a_and_clicks_activate_and_back_closes() {
    let (mut stack, mut host) = stack(0);
    stack.key(&mut host, &key(KeyCode::Enter), SIZE);
    assert!(host.log.is_empty(), "nothing focused yet");
    stack.key(&mut host, &key(KeyCode::Tab), SIZE);
    stack.key(&mut host, &key(KeyCode::Enter), SIZE);
    stack.key(&mut host, &key(KeyCode::Space), SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    let repeat = Key { repeat: true, ..key(KeyCode::Enter) };
    stack.key(&mut host, &repeat, SIZE);
    assert_eq!(host.log, ["A", "A", "A"], "a held key doesn't repeat activation");
    stack.mouse_moved(&mut host, (15.0, 15.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    stack.mouse_moved(&mut host, (235.0, 45.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    assert_eq!(host.log.len(), 4, "disabled widgets ignore clicks");
    assert_eq!(host.clicks, 4, "every activation clicks");
    // B opens a second screen; Escape and the controller's B both go back.
    stack.mouse_moved(&mut host, (125.0, 15.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    assert_eq!(stack.entries.len(), 2);
    stack.key(&mut host, &key(KeyCode::Escape), SIZE);
    assert_eq!(stack.entries.len(), 1);
    stack.nav(&mut host, Nav::Back, SIZE);
    assert!(!stack.is_open());
}

#[test]
fn screens_keep_their_focus_while_another_is_on_top() {
    let (mut stack, mut host) = stack(0);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Right, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert_eq!(stack.entries.len(), 2);
    assert_eq!(focused(&stack), None, "a new screen starts unfocused");
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(focused(&stack), Some(1), "back on B");
}

#[test]
fn sliders_step_with_arrows_and_follow_the_mouse() {
    let (mut stack, mut host) = stack(0);
    for _ in 0..3 {
        stack.key(&mut host, &key(KeyCode::Tab), SIZE);
    }
    stack.key(&mut host, &key(KeyCode::ArrowRight), SIZE);
    stack.nav(&mut host, Nav::Right, SIZE);
    assert!((host.volume - 0.2).abs() < 1e-6, "{}", host.volume);
    stack.key(&mut host, &key(KeyCode::ArrowLeft), SIZE);
    assert!((host.volume - 0.1).abs() < 1e-6);
    for _ in 0..5 {
        stack.nav(&mut host, Nav::Left, SIZE);
    }
    assert_eq!(host.volume, 0.0, "clamped at the bottom");
    assert_eq!(focused(&stack), Some(2), "left and right don't leave a slider");
    // Press on the track and drag past the end.
    stack.mouse_moved(&mut host, (60.0, 50.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    assert!((host.volume - 0.5).abs() < 1e-6, "{}", host.volume);
    stack.mouse_moved(&mut host, (500.0, 200.0), SIZE);
    assert_eq!(host.volume, 1.0);
    stack.mouse_button(&mut host, MouseButton::Left, false, SIZE);
    stack.mouse_moved(&mut host, (10.0, 50.0), SIZE);
    assert_eq!(host.volume, 1.0, "released: no longer dragging");
}

#[test]
fn sliders_reached_by_arrows_wait_for_enter_before_adjusting() {
    let (mut stack, mut host) = stack(0);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Down, SIZE);
    assert_eq!(focused(&stack), Some(2), "on the slider");
    stack.nav(&mut host, Nav::Right, SIZE);
    assert_eq!((focused(&stack), host.volume), (Some(3), 0.0), "Right moves on, as in Java");
    stack.nav(&mut host, Nav::Left, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    stack.nav(&mut host, Nav::Right, SIZE);
    assert_eq!((focused(&stack), host.volume), (Some(2), 0.1), "A starts adjusting");
    assert!(host.log.is_empty() && host.clicks == 0, "a slider doesn't activate or click");
    stack.key(&mut host, &key(KeyCode::Enter), SIZE);
    stack.key(&mut host, &key(KeyCode::ArrowRight), SIZE);
    assert_eq!((focused(&stack), host.volume), (Some(3), 0.1), "Enter stops adjusting");
}

#[test]
fn text_fields_edit_select_and_use_the_clipboard() {
    let (mut stack, mut host) = stack(0);
    stack.mouse_moved(&mut host, (150.0, 50.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    assert_eq!(focused(&stack), Some(3), "clicking a field focuses it");
    stack.key(&mut host, &Key::typed("hello world!"), SIZE);
    assert_eq!(field_text(&mut stack, &host), "hello worl", "ten characters at most");
    let shift_left = Key { shift: true, ..key(KeyCode::ArrowLeft) };
    for _ in 0..4 {
        stack.key(&mut host, &shift_left, SIZE);
    }
    let cmd = |code| Key { command: true, ..key(code) };
    stack.key(&mut host, &cmd(KeyCode::KeyX), SIZE);
    assert_eq!(field_text(&mut stack, &host), "hello ");
    stack.key(&mut host, &key(KeyCode::Home), SIZE);
    stack.key(&mut host, &cmd(KeyCode::KeyV), SIZE);
    assert_eq!(field_text(&mut stack, &host), "worlhello ");
    stack.key(&mut host, &cmd(KeyCode::KeyA), SIZE);
    stack.key(&mut host, &cmd(KeyCode::KeyC), SIZE);
    stack.key(&mut host, &Key::typed("é?"), SIZE);
    assert_eq!(field_text(&mut stack, &host), "?", "typing replaces the selection; non-ASCII is dropped");
    stack.key(&mut host, &key(KeyCode::Backspace), SIZE);
    stack.key(&mut host, &cmd(KeyCode::KeyV), SIZE);
    assert_eq!(field_text(&mut stack, &host), "worlhello ");
    let word_back = Key { alt: true, ..key(KeyCode::Backspace) };
    stack.key(&mut host, &word_back, SIZE);
    assert_eq!(field_text(&mut stack, &host), "");
    // Space types in a field instead of activating, Enter activates it, and
    // Up and Down leave it.
    stack.key(&mut host, &Key { text: Some(" ".into()), ..key(KeyCode::Space) }, SIZE);
    assert_eq!(field_text(&mut stack, &host), " ");
    stack.key(&mut host, &key(KeyCode::Enter), SIZE);
    assert_eq!(host.log, ["Name"]);
    stack.key(&mut host, &key(KeyCode::ArrowUp), SIZE);
    assert_eq!(focused(&stack), Some(1));
}

#[test]
fn the_caret_moves_by_characters_words_and_ends() {
    let mut clip = Memory::default();
    let mut field = TextField::with_text("one two three", 32);
    let k = |code| Key::new(code);
    assert_eq!(field.cursor(), 13);
    field.key(&Key { command: true, ..k(KeyCode::ArrowLeft) }, &mut clip);
    assert_eq!(field.cursor(), 8);
    field.key(&Key { alt: true, ..k(KeyCode::ArrowLeft) }, &mut clip);
    assert_eq!(field.cursor(), 4);
    field.key(&Key { shift: true, ..k(KeyCode::End) }, &mut clip);
    assert_eq!(field.selected(), "two three");
    field.key(&k(KeyCode::ArrowLeft), &mut clip);
    assert_eq!((field.cursor(), field.selected()), (4, ""), "left collapses a selection to its start");
    field.key(&k(KeyCode::Delete), &mut clip);
    assert_eq!(field.text(), "one wo three");
    field.key(&Key { command: true, ..k(KeyCode::Delete) }, &mut clip);
    assert_eq!(field.text(), "one three");
    // Clicks land on the nearest character boundary.
    field.click(4.0);
    assert_eq!(field.cursor(), 0);
    field.click(4.0 + Ui::text_width("one") + 1.0);
    assert_eq!(field.cursor(), 3);
    field.click(1000.0);
    assert_eq!(field.cursor(), 9);
}

#[test]
fn lists_scroll_to_keep_focus_in_view_and_hide_rows_outside() {
    let (mut stack, mut host) = stack(10);
    let shape = |stack: &Stack<TestHost>, host: &TestHost| {
        let e = stack.entries.last().unwrap();
        e.screen.shape(host, SIZE, e.scroll)
    };
    let s = shape(&stack, &host);
    let shown: Vec<bool> = s.slots[5..15].iter().map(|s| s.shown).collect();
    // A 150-pixel list fits six 25-pixel rows.
    assert_eq!(shown, [true, true, true, true, true, true, false, false, false, false]);
    assert!(s.max_scroll > 0.0);
    // Tabbing to the last row scrolls it into view.
    for _ in 0..14 {
        stack.key(&mut host, &key(KeyCode::Tab), SIZE);
    }
    assert_eq!(focused(&stack), Some(14));
    let s = shape(&stack, &host);
    assert!(s.slots[14].shown && !s.slots[5].shown, "scrolled to the last row");
    // A hidden row can't be clicked through.
    stack.wheel(&mut host, 100.0, SIZE);
    assert_eq!(stack.entries.last().unwrap().scroll, 0.0, "clamped at the top");
    stack.wheel(&mut host, -100.0, SIZE);
    assert_eq!(stack.entries.last().unwrap().scroll, s.max_scroll, "clamped at the bottom");
    stack.mouse_moved(&mut host, (20.0, 105.0), SIZE);
    stack.mouse_button(&mut host, MouseButton::Left, true, SIZE);
    assert_eq!(host.log, ["Row(4)"], "the row now at the top of the list");
    // A step's Out reaches the caller, and Close empties the stack.
    stack.mouse_moved(&mut host, (20.0, 275.0), SIZE);
    assert_eq!(stack.mouse_button(&mut host, MouseButton::Left, true, SIZE), Some("quit"));
    stack.key(&mut host, &key(KeyCode::Tab), SIZE);
    for _ in 0..20 {
        stack.nav(&mut host, Nav::Down, SIZE);
    }
    stack.nav(&mut host, Nav::Up, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert!(!stack.is_open(), "row 9 closes everything");
}

#[test]
fn confirm_runs_its_action_on_the_screen_below() {
    let (mut stack, mut host) = stack(0);
    stack.open(Confirm::new("Sure?", vec!["Really?".into()], "Yes", |h: &mut TestHost| {
        h.log.push("deleted".into());
        Step::push(Notice::new("Done", vec!["It's gone.".into()]))
    }));
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Right, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert_eq!(stack.entries.len(), 1, "Cancel goes back");
    assert!(host.log.is_empty());
    stack.open(Confirm::new("Sure?", vec![], "Yes", |h: &mut TestHost| {
        h.log.push("deleted".into());
        Step::push(Notice::new("Done", vec!["It's gone.".into()]))
    }));
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    assert_eq!(host.log, ["deleted"]);
    assert_eq!(stack.entries.len(), 2, "the notice replaced the question");
    // Notices focus their button, so one press dismisses them.
    stack.key(&mut host, &key(KeyCode::Enter), SIZE);
    assert_eq!(stack.entries.len(), 1);
}

#[test]
fn drawing_every_screen_state_is_safe() {
    let (mut stack, mut host) = stack(10);
    let mut ui = Ui::with_scale(800.0, 600.0, 2.0);
    stack.draw(&host, &mut ui);
    stack.mouse_moved(&mut host, (235.0, 45.0), SIZE);
    stack.draw(&host, &mut ui);
    stack.key(&mut host, &key(KeyCode::Tab), SIZE);
    stack.draw(&host, &mut ui);
    stack.open(Notice::new("Couldn't open the world", vec!["x".repeat(400)]));
    stack.draw(&host, &mut ui);
    assert!(!ui.verts.is_empty());
}

#[test]
fn controller_buttons_map_to_menu_navigation() {
    use gilrs::Button as B;
    assert_eq!(Nav::from_pad(B::DPadUp), Some(Nav::Up));
    assert_eq!(Nav::from_pad(B::South), Some(Nav::Activate));
    assert_eq!(Nav::from_pad(B::East), Some(Nav::Back));
    assert_eq!(Nav::from_pad(B::Start), None, "Start joins split-screen");
}

#[test]
fn text_helpers_fit_and_wrap() {
    assert_eq!(fit("short", 100.0), "short");
    let long = fit("Survival, in the Nether - 12 minutes ago (My_Long_World_Name)", 200.0);
    assert!(long.ends_with("...") && Ui::text_width(&long) <= 200.0, "{long}");
    let lines = wrap("the quick brown fox jumps over the lazy dog", 60.0);
    assert!(lines.len() > 1 && lines.iter().all(|l| Ui::text_width(l) <= 60.0), "{lines:?}");
}
