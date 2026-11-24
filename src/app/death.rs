//! v0.18 Track 2: Java 26.3's death screen (`DeathScreen`) on the menu
//! toolkit, for the host's own player on mouse and keyboard and on a
//! controller that hasn't joined as a player.
//!
//! "You Died!" offers Respawn and Title Screen; Hardcore's "Game Over!"
//! offers Spectate World and Title Screen. Both buttons wait 20 client
//! ticks (a second) before they take a press, Escape does nothing, and
//! Title Screen asks "Are you sure you want to quit?" first, except in
//! Hardcore. The confirmation's buttons wait 20 ticks too, and once they
//! are live Escape answers Respawn (`ConfirmScreen.keyPressed`).
//!
//! The screens sit on their own stack ([`Game::death`]), not the pause
//! menu's: Java's death screen doesn't pause the game (`isPauseScreen`
//! is false), and it doesn't blur the world behind it.

use std::time::{Duration, Instant};

use winit::event::{KeyEvent, MouseButton};

use super::gui::{self, Key, Nav, Screen, Size, Stack, Step, Widget, style};
use super::menu::MenuAction;
use super::{Game, GameMode};
use crate::render::ui::{Ui, WHITE};

/// What the death screens need from the game (tests use their own).
pub(super) trait DeathHost: gui::Host<Out = MenuAction> {
    /// Respawn, or in Hardcore Spectate World.
    fn respawn_from_death(&mut self);
    /// The death message ("Player fell from a high place") and the score.
    fn death_text(&self) -> (Option<String>, u32);
}

impl DeathHost for Game {
    fn respawn_from_death(&mut self) {
        Game::respawn_from_death(self);
    }

    fn death_text(&self) -> (Option<String>, u32) {
        let cause = self.session.vitals.death.as_ref().map(|cause| format!("Player {cause}"));
        (cause, self.session.vitals.xp.total)
    }
}

/// `DeathScreen.tick`: the buttons come alive on the 20th client tick.
pub(super) const DELAY_TICKS: u32 = 20;
/// [`DELAY_TICKS`] at Java's 20 client ticks a second.
pub(super) const DELAY: Duration = Duration::from_millis(DELAY_TICKS as u64 * 50);
/// How long a LAN guest's death screen stays away after Respawn while the
/// host's answer is on its way.
const RESPAWN_WAIT: Duration = Duration::from_secs(2);
/// `DeathScreen`'s title scale and positions, in UI pixels.
const TITLE_SCALE: f32 = 2.0;
const TITLE_Y: f32 = 30.0 * TITLE_SCALE;
const CAUSE_Y: f32 = 85.0;
const SCORE_Y: f32 = 100.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DeathId {
    /// Respawn, or Spectate World in Hardcore.
    Respawn,
    TitleScreen,
}

/// The death screen for a player who died in a world that is `hardcore`.
pub(super) struct DeathScreen {
    hardcore: bool,
    opened: Instant,
    /// Respawn was pressed (Java greys the button until the screen goes).
    respawning: bool,
}

impl DeathScreen {
    pub fn new(hardcore: bool) -> Self {
        Self::opened_at(hardcore, Instant::now())
    }

    /// A death screen that opened at `opened` (tests start it in the past).
    pub fn opened_at(hardcore: bool, opened: Instant) -> Self {
        Self { hardcore, opened, respawning: false }
    }

    fn live(&self) -> bool {
        self.opened.elapsed() >= DELAY
    }

    /// `deathScreen.title` or `deathScreen.title.hardcore`.
    pub fn title_text(hardcore: bool) -> &'static str {
        if hardcore { "Game Over!" } else { "You Died!" }
    }
}

/// The two button rects: 200 x 20, centred, at a quarter of the height
/// plus 72 and 96 (`DeathScreen.init`).
pub(super) fn button_rects((sw, sh): Size) -> [gui::Rect; 2] {
    let x = (sw / 2.0).floor() - 100.0;
    let top = (sh / 4.0).floor();
    [[x, top + 72.0, 200.0, 20.0], [x, top + 96.0, 200.0, 20.0]]
}

impl<H: DeathHost> Screen<H> for DeathScreen {
    type Id = DeathId;

    /// The title is drawn twice its size in [`Screen::draw`].
    fn title(&self, _: &H) -> String {
        String::new()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<DeathId>> {
        let [respawn, title] = button_rects(size);
        let live = self.live();
        let first = if self.hardcore { "Spectate World" } else { "Respawn" };
        vec![
            Widget::button(DeathId::Respawn, respawn, first).enabled(live && !self.respawning),
            Widget::button(DeathId::TitleScreen, title, "Title Screen").enabled(live),
        ]
    }

    fn activate(&mut self, host: &mut H, id: DeathId) -> Step<H> {
        if !self.live() {
            return Step::Stay;
        }
        match id {
            DeathId::Respawn => {
                self.respawning = true;
                host.respawn_from_death();
                Step::Close
            }
            DeathId::TitleScreen if self.hardcore => Step::Out(MenuAction::Quit),
            DeathId::TitleScreen => Step::push(QuitConfirm::new()),
        }
    }

    /// `shouldCloseOnEsc` is false.
    fn back(&mut self, _: &mut H) -> Step<H> {
        Step::Stay
    }

    fn background(&self, _: &H, ui: &mut Ui, _: Size) {
        style::backdrop(ui, style::DEATH_VEIL_TOP, style::DEATH_VEIL_BOTTOM);
    }

    fn blurs(&self) -> bool {
        false
    }

    fn draw(&self, host: &H, ui: &mut Ui, (sw, _): Size, _: f32) {
        let title = Self::title_text(self.hardcore);
        let tw = Ui::text_width(title) * TITLE_SCALE;
        ui.text_scaled(((sw - tw) / 2.0).floor(), TITLE_Y, title, WHITE, TITLE_SCALE);
        let (cause, score) = host.death_text();
        if let Some(msg) = cause {
            ui.text(((sw - Ui::text_width(&msg)) / 2.0).floor(), CAUSE_Y, &msg, WHITE);
        }
        // `deathScreen.score.value`, the number in yellow.
        let score = score.to_string();
        let x = ((sw - Ui::text_width(&format!("Score: {score}"))) / 2.0).floor();
        let n = x + Ui::text_width("Score: ") + 1.0;
        ui.text(x, SCORE_Y, "Score: ", WHITE);
        ui.text(n, SCORE_Y, &score, style::srgb(style::token::YELLOW_TEXT));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ConfirmId {
    /// "Title Screen": leave the world.
    Quit,
    /// "Respawn".
    Respawn,
}

/// `DeathScreen.TitleConfirmScreen`: "Are you sure you want to quit?"
/// over the death screen's red, with Title Screen and Respawn.
pub(super) struct QuitConfirm {
    opened: Instant,
}

impl QuitConfirm {
    pub fn new() -> Self {
        Self::opened_at(Instant::now())
    }

    pub fn opened_at(opened: Instant) -> Self {
        Self { opened }
    }

    /// `ConfirmScreen.setDelay(20)`.
    fn live(&self) -> bool {
        self.opened.elapsed() >= DELAY
    }
}

/// `ConfirmScreen`'s layout: the title, then (after Java's 8-pixel
/// spacing, the empty message and 16 more pixels above the row) two
/// 150-pixel buttons 4 apart, the whole column centred on screen.
pub(super) fn confirm_layout((sw, sh): Size) -> (f32, [gui::Rect; 2]) {
    const TITLE_H: f32 = 9.0;
    let height = TITLE_H + 8.0 + 8.0 + 16.0 + 20.0;
    let top = ((sh - height) / 2.0).floor();
    let y = top + TITLE_H + 8.0 + 8.0 + 16.0;
    let left = (sw / 2.0).floor() - 152.0;
    (top, [[left, y, 150.0, 20.0], [left + 154.0, y, 150.0, 20.0]])
}

impl<H: DeathHost> Screen<H> for QuitConfirm {
    type Id = ConfirmId;

    fn title(&self, _: &H) -> String {
        "Are you sure you want to quit?".into()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<ConfirmId>> {
        let (_, [quit, respawn]) = confirm_layout(size);
        let live = self.live();
        vec![
            Widget::button(ConfirmId::Quit, quit, "Title Screen").enabled(live),
            Widget::button(ConfirmId::Respawn, respawn, "Respawn").enabled(live),
        ]
    }

    fn activate(&mut self, host: &mut H, id: ConfirmId) -> Step<H> {
        if !self.live() {
            return Step::Stay;
        }
        match id {
            ConfirmId::Quit => Step::Out(MenuAction::Quit),
            ConfirmId::Respawn => {
                host.respawn_from_death();
                Step::Close
            }
        }
    }

    /// Escape answers Respawn once the buttons are live.
    fn back(&mut self, host: &mut H) -> Step<H> {
        self.activate(host, ConfirmId::Respawn)
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        confirm_layout(size).0
    }

    fn background(&self, _: &H, ui: &mut Ui, _: Size) {
        style::backdrop(ui, style::DEATH_VEIL_TOP, style::DEATH_VEIL_BOTTOM);
    }

    fn blurs(&self) -> bool {
        false
    }
}

impl Game {
    /// Opens the death screen when the host's player is dead and no
    /// immediate respawn is due, and closes it once they're alive.
    pub(super) fn sync_death_screen(&mut self) {
        let dead = self.session.vitals.is_dead();
        // A LAN guest's respawn waits for the host: don't reopen the screen
        // meanwhile, unless the host hasn't answered for a while.
        let waiting = self.death_respawn_sent.is_some_and(|t| t.elapsed() < RESPAWN_WAIT);
        if !dead {
            self.death_respawn_sent = None;
        }
        if dead && !self.respawn_due && !waiting && !self.death.is_open() && self.credits.is_none() {
            self.death.open(DeathScreen::new(self.hardcore));
        } else if !dead && self.death.is_open() {
            self.death.close();
        }
    }

    /// Respawn (or Spectate World) from the death screen: the shared
    /// respawn step, then, in Hardcore, Spectator mode, as the server does
    /// for `ServerboundClientCommandPacket.PERFORM_RESPAWN`.
    pub(super) fn respawn_from_death(&mut self) {
        if !self.session.vitals.is_dead() {
            return;
        }
        self.respawn();
        if self.hardcore && self.lan.client.is_none() && self.session.mode != GameMode::Spectator {
            self.set_mode(GameMode::Spectator);
        }
        if self.session.vitals.is_dead() {
            self.death_respawn_sent = Some(Instant::now());
        }
    }

    /// Runs `f` on the death screens with the game as their host.
    fn with_death<T>(&mut self, f: impl FnOnce(&mut Stack<Game>, &mut Game, Size) -> T) -> T {
        let mut stack = std::mem::take(&mut self.death);
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        let out = f(&mut stack, self, (w as f32 / scale, h as f32 / scale));
        // A respawn closed the screens; a new death since reopens them.
        if !self.death.is_open() {
            self.death = stack;
        }
        out
    }

    pub(super) fn death_key(&mut self, event: &KeyEvent) -> Option<MenuAction> {
        let key = Key::from_event(event, self.modifiers)?;
        self.with_death(|stack, game, size| stack.key(game, &key, size))
    }

    pub(super) fn death_click(&mut self, button: MouseButton, pressed: bool) -> Option<MenuAction> {
        self.with_death(|stack, game, size| stack.mouse_button(game, button, pressed, size))
    }

    pub(super) fn death_cursor_moved(&mut self) {
        let scale = self.ui_scale();
        let pos = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        self.with_death(|stack, game, size| stack.mouse_moved(game, pos, size));
    }

    pub(super) fn death_nav(&mut self, nav: Nav) -> Option<MenuAction> {
        self.with_death(|stack, game, size| stack.nav(game, nav, size))
    }

    /// Once a frame: the screens' own ticks.
    pub(super) fn death_tick(&mut self) -> Option<MenuAction> {
        if !self.death.is_open() {
            return None;
        }
        self.with_death(|stack, game, size| stack.tick(game, size))
    }

    pub(super) fn death_ui(&self, ui: &mut Ui) {
        self.death.draw(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_sit_where_java_puts_them_at_every_scale() {
        // 1920 x 1080 at GUI scales 4, 3, 2 and 1, and 1280 x 720 at 3 and 2.
        for size in [(480.0, 270.0), (640.0, 360.0), (960.0, 540.0), (1920.0, 1080.0), (426.0, 240.0)] {
            let [respawn, title] = button_rects(size);
            assert_eq!(respawn[2..], [200.0, 20.0]);
            assert_eq!(respawn[0] + 100.0, (size.0 / 2.0).floor(), "centred at {size:?}");
            assert_eq!(respawn[1], (size.1 / 4.0).floor() + 72.0);
            assert_eq!(title[1] - respawn[1], 24.0);
            assert!(title[1] + title[3] <= size.1, "on screen at {size:?}");
            assert!(respawn[1] > SCORE_Y + 9.0, "below the score at {size:?}");
            let (top, [quit, again]) = confirm_layout(size);
            assert_eq!(quit[1], again[1]);
            assert_eq!(again[0] - quit[0], 154.0, "150 wide, 4 apart");
            assert_eq!(quit[0] + 152.0, (size.0 / 2.0).floor());
            assert_eq!(quit[1] - top, 41.0, "title, 8, message, 8, 16 above the row");
            assert!(quit[1] + 20.0 <= size.1);
        }
    }

    use crate::app::gui::tests::Memory;
    use winit::keyboard::KeyCode;

    #[derive(Default)]
    struct Dead {
        respawns: u32,
        clicks: u32,
    }

    impl gui::Host for Dead {
        type Out = MenuAction;
        fn click(&mut self) {
            self.clicks += 1;
        }
        fn in_world(&self) -> bool {
            true
        }
    }

    impl DeathHost for Dead {
        fn respawn_from_death(&mut self) {
            self.respawns += 1;
        }
        fn death_text(&self) -> (Option<String>, u32) {
            (Some("Player fell from a high place".into()), 7)
        }
    }

    const SIZE: Size = (480.0, 270.0);
    const LONG_AGO: Duration = Duration::from_secs(5);

    fn stack(screen: impl Screen<Dead>) -> Stack<Dead> {
        let mut stack = Stack::with_clipboard(Box::new(Memory::default()));
        stack.open(screen);
        stack
    }

    fn centre(r: gui::Rect) -> (f32, f32) {
        (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
    }

    fn click(stack: &mut Stack<Dead>, host: &mut Dead, at: (f32, f32)) -> Option<MenuAction> {
        stack.mouse_moved(host, at, SIZE);
        stack.mouse_button(host, MouseButton::Left, true, SIZE)
    }

    #[test]
    fn buttons_wait_a_second_then_respawn_on_every_input_path() {
        let [respawn, title] = button_rects(SIZE);
        // Mouse: nothing happens before the delay; Respawn after it.
        let mut host = Dead::default();
        let mut s = stack(DeathScreen::new(false));
        assert_eq!(click(&mut s, &mut host, centre(respawn)), None);
        assert_eq!(click(&mut s, &mut host, centre(title)), None);
        assert_eq!((host.respawns, s.depth()), (0, 1), "disabled for 20 ticks");
        let mut s = stack(DeathScreen::opened_at(false, Instant::now() - LONG_AGO));
        click(&mut s, &mut host, centre(respawn));
        assert_eq!((host.respawns, s.is_open()), (1, false));
        // Keyboard: Escape does nothing; Tab then Enter respawns.
        let mut host = Dead::default();
        let mut s = stack(DeathScreen::opened_at(false, Instant::now() - LONG_AGO));
        s.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
        assert_eq!(s.depth(), 1, "shouldCloseOnEsc is false");
        s.key(&mut host, &Key::new(KeyCode::Tab), SIZE);
        s.key(&mut host, &Key::new(KeyCode::Enter), SIZE);
        assert_eq!((host.respawns, s.is_open()), (1, false));
        // A controller that hasn't joined: B does nothing, D-pad and A respawn.
        let mut host = Dead::default();
        let mut s = stack(DeathScreen::opened_at(false, Instant::now() - LONG_AGO));
        s.nav(&mut host, Nav::Back, SIZE);
        assert_eq!(s.depth(), 1);
        s.nav(&mut host, Nav::Down, SIZE);
        s.nav(&mut host, Nav::Activate, SIZE);
        assert_eq!((host.respawns, s.is_open()), (1, false));
        // Before the delay the controller can't land on a button either.
        let mut s = stack(DeathScreen::new(false));
        s.nav(&mut host, Nav::Down, SIZE);
        s.nav(&mut host, Nav::Activate, SIZE);
        assert_eq!((host.respawns, s.depth()), (1, 1));
    }

    #[test]
    fn title_screen_asks_first_except_in_hardcore() {
        let [_, title] = button_rects(SIZE);
        let mut host = Dead::default();
        let mut s = stack(DeathScreen::opened_at(false, Instant::now() - LONG_AGO));
        assert_eq!(click(&mut s, &mut host, centre(title)), None);
        assert_eq!(s.depth(), 2, "the quit confirmation");
        // Its own 20-tick delay: neither button nor Escape works yet.
        let (_, [quit, again]) = confirm_layout(SIZE);
        assert_eq!(click(&mut s, &mut host, centre(quit)), None);
        s.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
        assert_eq!((s.depth(), host.respawns), (2, 0));
        // Live: Title Screen quits, Respawn (or Escape) respawns.
        let mut s = stack(QuitConfirm::opened_at(Instant::now() - LONG_AGO));
        assert_eq!(click(&mut s, &mut host, centre(quit)), Some(MenuAction::Quit));
        let mut s = stack(QuitConfirm::opened_at(Instant::now() - LONG_AGO));
        click(&mut s, &mut host, centre(again));
        assert_eq!((host.respawns, s.is_open()), (1, false));
        let mut s = stack(QuitConfirm::opened_at(Instant::now() - LONG_AGO));
        s.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
        assert_eq!((host.respawns, s.is_open()), (2, false), "Escape answers Respawn");
        // Hardcore: Title Screen leaves at once; Spectate World respawns.
        let mut host = Dead::default();
        let mut s = stack(DeathScreen::opened_at(true, Instant::now() - LONG_AGO));
        assert_eq!(click(&mut s, &mut host, centre(title)), Some(MenuAction::Quit));
        let mut s = stack(DeathScreen::opened_at(true, Instant::now() - LONG_AGO));
        let [spectate, _] = button_rects(SIZE);
        click(&mut s, &mut host, centre(spectate));
        assert_eq!(host.respawns, 1);
    }

    #[test]
    fn hardcore_offers_spectate_world_and_neither_screen_blurs() {
        let host = Dead::default();
        let label = |hardcore| match &Screen::<Dead>::widgets(&DeathScreen::new(hardcore), &host, SIZE)[0].kind {
            gui::Kind::Button(l) => l.clone(),
            _ => unreachable!(),
        };
        assert_eq!((label(false).as_str(), label(true).as_str()), ("Respawn", "Spectate World"));
        assert!(!stack(DeathScreen::new(false)).blurs());
        assert!(!stack(QuitConfirm::new()).blurs());
    }

    /// v0.18 Track 2: the shared UI's Java numbers against the fixture
    /// `tools/extract_java_ui_fixtures.py` writes from the pinned client.
    #[test]
    fn matches_the_java_fixture() {
        use crate::app::{anvil, options, settings};
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/ui_screens.json")).unwrap();
        let n = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let hex = |v: &serde_json::Value| u32::from_str_radix(&v.as_str().unwrap()[2..], 16).unwrap();
        let d = &f["death_screen"];
        assert_eq!(d["title"], DeathScreen::title_text(false));
        assert_eq!(d["title_hardcore"], DeathScreen::title_text(true));
        let host = Dead::default();
        let labels = |hardcore| -> Vec<String> {
            Screen::<Dead>::widgets(&DeathScreen::new(hardcore), &host, SIZE)
                .into_iter()
                .map(|w| match w.kind {
                    gui::Kind::Button(l) => l,
                    _ => unreachable!(),
                })
                .collect()
        };
        assert_eq!(labels(false), [d["respawn"].as_str().unwrap(), d["title_screen"].as_str().unwrap()]);
        assert_eq!(labels(true), [d["spectate"].as_str().unwrap(), d["title_screen"].as_str().unwrap()]);
        assert_eq!(d["score"], "Score: %s");
        let [first, second] = button_rects((640.0, 360.0));
        assert_eq!(320.0 - first[0], n(&d["button_half_width"]));
        assert_eq!(
            [first[1] - 90.0, second[1] - 90.0],
            [n(&d["button_quarter_rows"][0]), n(&d["button_quarter_rows"][1])]
        );
        assert_eq!(first[2..], [n(&d["button_size"][0]), n(&d["button_size"][1])]);
        assert_eq!(DELAY_TICKS as f32, n(&d["delay_ticks"]));
        assert_eq!(TITLE_SCALE, n(&d["title_scale"]));
        assert_eq!(TITLE_Y, n(&d["title_y_scaled"]) * TITLE_SCALE);
        assert_eq!((CAUSE_Y, SCORE_Y), (n(&d["cause_y"]), n(&d["score_y"])));
        assert_eq!(
            [style::DEATH_VEIL_TOP, style::DEATH_VEIL_BOTTOM],
            [hex(&d["background"][0]), hex(&d["background"][1])]
        );
        let q = &f["quit_confirm"];
        assert_eq!(Screen::<Dead>::title(&QuitConfirm::new(), &host), q["title"].as_str().unwrap());
        assert_eq!(q["delay_ticks"], 20);
        let (top, [yes, no]) = confirm_layout(SIZE);
        assert_eq!(yes[1] - top, 9.0 + 2.0 * n(&q["spacing"]) + n(&q["button_padding_top"]));
        assert_eq!(no[0] - yes[0] - yes[2], n(&q["button_spacing"]));
        assert_eq!(yes[2..], [n(&q["button_size"][0]), n(&q["button_size"][1])]);
        let b = &f["menu_blur"];
        assert_eq!(settings::MENU_BLUR, (b["min"].as_u64().unwrap() as u32, b["max"].as_u64().unwrap() as u32));
        assert_eq!(settings::MENU_BLUR_DEFAULT as u64, b["default"].as_u64().unwrap());
        assert!(settings::Settings::default().serialize().contains(&format!("{}=5\n", b["key"].as_str().unwrap())));
        let label = b["label"].as_str().unwrap();
        assert_eq!(options::menu_blur_label(0), format!("{label}: {}", b["off"].as_str().unwrap()));
        assert_eq!(options::menu_blur_label(7), format!("{label}: 7"));
        assert_eq!(b["passes"].as_array().unwrap().len(), crate::render::blur_passes());
        let bg = &f["backgrounds"];
        assert_eq!(
            [style::CONTAINER_VEIL_TOP, style::CONTAINER_VEIL_BOTTOM],
            [hex(&bg["container"][0]), hex(&bg["container"][1])]
        );
        assert_eq!(style::MENU_VEIL >> 24, bg["in_world_menu_alpha"].as_u64().unwrap() as u32);
        let a = &f["anvil"];
        assert_eq!(a["title"], "Repair & Name");
        assert_eq!(anvil::NAME_BOX.to_vec(), a["field_sprite"].as_array().unwrap().iter().map(n).collect::<Vec<_>>());
        assert_eq!(
            (anvil::NAME_TEXT.0, anvil::NAME_TEXT.1, anvil::NAME_TEXT.2),
            (n(&a["field"][0]), n(&a["field"][1]), n(&a["field"][2]))
        );
        assert_eq!(anvil::NAME_LENGTH as u64, a["field_max_length"].as_u64().unwrap());
        assert_eq!(a["cost"], "Enchantment Cost: %1$s");
        assert_eq!(a["too_expensive_at"], crate::enchant::TOO_EXPENSIVE);
        assert_eq!(hex(&a["cost_colour"]) & 0xFF_FFFF, style::token::COST_TEXT);
        assert_eq!(hex(&a["short_colour"]) & 0xFF_FFFF, style::token::COST_TEXT_SHORT);
    }

    #[test]
    fn labels_follow_the_mode() {
        assert_eq!(DeathScreen::title_text(false), "You Died!");
        assert_eq!(DeathScreen::title_text(true), "Game Over!");
        assert_eq!(DELAY, Duration::from_secs(1));
    }
}
