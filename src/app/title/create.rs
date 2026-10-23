//! Java 26.3's Create New World screen (`CreateWorldScreen`): a tab bar,
//! the current tab's settings, and Create New World and Cancel in the
//! footer. VoxelCraft has the Game tab's name, game mode (Survival,
//! Hardcore, Creative, cycled as in Java) and difficulty, and the World
//! tab's seed. Allow Commands, World Type, Customize, Generate Structures,
//! Bonus Chest and the More tab (Game Rules, Experiments, Data Packs) have
//! nothing behind them yet and are left out. Re-Create opens it filled from
//! an existing world.

use std::cell::RefCell;

use super::worlds::{Summary, folder_for, parse_seed, seed_text};
use super::{Action, NewWorld, TitleHost};
use crate::app::GameMode;
use crate::app::gui::{
    self, BUTTON_H, GREY, HOT_TEXT, Look, Rect, Screen, Size, Step, TextField, Widget, label_text, text_color,
};
use crate::render::ui::{Ui, WHITE};
use crate::simulation::difficulty::Difficulty;

const NAME_MAX: usize = 32;
const SEED_MAX: usize = 32;
/// Java's tab bar height and the footer's.
const TAB_H: f32 = 24.0;
const FOOTER: f32 = 33.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Tab {
    Game,
    World,
}

impl Tab {
    const ALL: [Tab; 2] = [Tab::Game, Tab::World];

    fn title(self) -> &'static str {
        match self {
            Tab::Game => "Game",
            Tab::World => "World",
        }
    }
}

/// Java's create-screen game modes, in its cycling order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Mode {
    Survival,
    Hardcore,
    Creative,
}

impl Mode {
    fn next(self) -> Self {
        match self {
            Mode::Survival => Mode::Hardcore,
            Mode::Hardcore => Mode::Creative,
            Mode::Creative => Mode::Survival,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Mode::Survival => "Survival",
            Mode::Hardcore => "Hardcore",
            Mode::Creative => "Creative",
        }
    }

    /// Java's `selectWorld.gameMode.*.info` tooltips.
    fn info(self) -> &'static str {
        match self {
            Mode::Survival => "Explore a mysterious world where you build, collect, craft, and fight monsters.",
            Mode::Hardcore => "Survival Mode locked to 'Hard' difficulty. You can't respawn if you die.",
            Mode::Creative => {
                "Create, build, and explore without limits. You can fly, have endless materials, and can't be hurt \
                 by monsters."
            }
        }
    }
}

/// Java's `options.difficulty.*.info` tooltips.
fn difficulty_info(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Peaceful => {
            "No hostile mobs and only some neutral mobs spawn. Hunger bar doesn't deplete and health replenishes \
             over time."
        }
        Difficulty::Easy => {
            "Hostile mobs spawn but deal less damage. Hunger bar depletes and drains health down to 5 hearts."
        }
        Difficulty::Normal => {
            "Hostile mobs spawn and deal standard damage. Hunger bar depletes and drains health down to half a heart."
        }
        Difficulty::Hard => "Hostile mobs spawn and deal more damage. Hunger bar depletes and drains all health.",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Tab(Tab),
    Name,
    Mode,
    Difficulty,
    Seed,
    Create,
    Cancel,
}

pub(in crate::app) struct CreateWorld {
    tab: Tab,
    name: TextField,
    seed: TextField,
    mode: Mode,
    difficulty: Difficulty,
    /// The name and save folder last shown in the tooltip, so drawing
    /// doesn't look at the disk every frame.
    shown_folder: RefCell<(String, String)>,
}

impl CreateWorld {
    pub fn new() -> Self {
        Self {
            tab: Tab::Game,
            name: TextField::with_text("New World", NAME_MAX),
            seed: TextField::new(SEED_MAX).hint("Leave blank for a random seed"),
            mode: Mode::Survival,
            difficulty: Difficulty::Normal,
            shown_folder: RefCell::default(),
        }
    }

    /// Java's Re-Create: the same name, seed, game mode and difficulty.
    pub fn recreate(world: &Summary) -> Self {
        let mode = match (world.hardcore, world.mode) {
            (true, _) => Mode::Hardcore,
            (false, GameMode::Creative) => Mode::Creative,
            (false, _) => Mode::Survival,
        };
        Self {
            name: TextField::with_text(&world.name, NAME_MAX),
            seed: TextField::with_text(&seed_text(world.seed), SEED_MAX).hint("Leave blank for a random seed"),
            mode,
            difficulty: world.difficulty,
            ..Self::new()
        }
    }

    /// Hardcore locks the difficulty to Hard, as Java's does.
    fn difficulty(&self) -> Difficulty {
        if self.mode == Mode::Hardcore { Difficulty::Hard } else { self.difficulty }
    }

    fn name(&self) -> String {
        match self.name.text().trim() {
            "" => "New World".to_string(),
            name => name.to_string(),
        }
    }

    fn folder<H: TitleHost>(&self, host: &H) -> String {
        folder_for(&self.name(), |d| host.saves_dir().join(d).exists())
    }

    /// [`Self::folder`] for the tooltip, worked out again only when the
    /// name changes.
    fn tooltip_folder<H: TitleHost>(&self, host: &H) -> String {
        let name = self.name();
        let mut shown = self.shown_folder.borrow_mut();
        if shown.1.is_empty() || shown.0 != name {
            *shown = (name, self.folder(host));
        }
        shown.1.clone()
    }

    fn world<H: TitleHost>(&self, host: &H) -> Action {
        let new = NewWorld {
            name: self.name(),
            seed: parse_seed(self.seed.text()),
            mode: if self.mode == Mode::Creative { GameMode::Creative } else { GameMode::Survival },
            difficulty: self.difficulty(),
            hardcore: self.mode == Mode::Hardcore,
        };
        Action::Play(self.folder(host), Some(new))
    }

    /// Java's tab buttons: `min(400, width) - 28` split evenly, centred.
    fn tab_rects(sw: f32) -> Vec<Rect> {
        let n = Tab::ALL.len() as f32;
        let w = (((sw.min(400.0) - 28.0) / n) / 2.0).floor() * 2.0;
        let left = ((sw - w * n) / 2.0).floor();
        (0..Tab::ALL.len()).map(|i| [left + i as f32 * w, 0.0, w, TAB_H]).collect()
    }

    /// The tab's content block, placed as Java's grid tabs are: centred
    /// across, one sixth of the way down the space between bar and footer.
    fn content_top(&self, (_, sh): Size) -> f32 {
        let height = match self.tab {
            Tab::Game => 33.0 + 8.0 + BUTTON_H + 8.0 + BUTTON_H,
            Tab::World => 33.0,
        };
        let area = sh - TAB_H - FOOTER;
        (TAB_H + ((area - height) / 6.0).max(2.0)).floor()
    }
}

impl<H: TitleHost> Screen<H> for CreateWorld {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        // The tab bar takes the header's place.
        String::new()
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let (sw, sh) = size;
        let cx = (sw / 2.0).floor();
        let mut out: Vec<Widget<Id>> =
            Tab::ALL.iter().zip(Self::tab_rects(sw)).map(|(&t, r)| Widget::custom(Id::Tab(t), r)).collect();
        let top = self.content_top(size);
        match self.tab {
            Tab::Game => {
                let folder = format!("Save folder: {}", self.tooltip_folder(host));
                out.push(Widget::field(Id::Name, [cx - 104.0, top + 13.0, 208.0, BUTTON_H]).tooltip(folder));
                let mode = format!("Game Mode: {}", self.mode.name());
                out.push(
                    Widget::button(Id::Mode, [cx - 105.0, top + 41.0, 210.0, BUTTON_H], mode).tooltip(self.mode.info()),
                );
                let difficulty = format!("Difficulty: {}", self.difficulty());
                out.push(
                    Widget::button(Id::Difficulty, [cx - 105.0, top + 69.0, 210.0, BUTTON_H], difficulty)
                        .enabled(self.mode != Mode::Hardcore)
                        .tooltip(difficulty_info(self.difficulty())),
                );
            }
            Tab::World => out.push(Widget::field(Id::Seed, [cx - 154.0, top + 13.0, 308.0, BUTTON_H])),
        }
        let y = (sh - FOOTER + (FOOTER - BUTTON_H) / 2.0).floor();
        out.push(Widget::button(Id::Create, [cx - 154.0, y, 150.0, BUTTON_H], "Create New World"));
        out.push(Widget::button(Id::Cancel, [cx + 4.0, y, 150.0, BUTTON_H], "Cancel"));
        out
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Tab(tab) => self.tab = tab,
            Id::Name | Id::Seed | Id::Create => return Step::Out(self.world(host)),
            Id::Mode => self.mode = self.mode.next(),
            Id::Difficulty => self.difficulty = self.difficulty.next(),
            Id::Cancel => return Step::Pop,
        }
        Step::Stay
    }

    /// Focusing a tab shows it, as Java's tab bar does.
    fn focus(&mut self, _: &mut H, id: Id) {
        if let Id::Tab(tab) = id {
            self.tab = tab;
        }
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        match id {
            Id::Name => Some(&mut self.name),
            Id::Seed => Some(&mut self.seed),
            _ => None,
        }
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        match id {
            Id::Name => Some(&self.name),
            Id::Seed => Some(&self.seed),
            _ => None,
        }
    }

    fn initial_focus(&self) -> Option<Id> {
        Some(Id::Name)
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let (sw, sh) = size;
        // Java's header and footer separators.
        let bar = Self::tab_rects(sw);
        let (first, last) = (bar[0][0], bar[bar.len() - 1][0] + bar[bar.len() - 1][2]);
        ui.rect(0.0, TAB_H - 2.0, first, 2.0, [0.0, 0.0, 0.0, 0.6]);
        ui.rect(last, TAB_H - 2.0, sw - last, 2.0, [0.0, 0.0, 0.0, 0.6]);
        ui.rect(0.0, sh - FOOTER, sw, 2.0, [0.0, 0.0, 0.0, 0.6]);
        let cx = (sw / 2.0).floor();
        let top = self.content_top(size);
        let label = match self.tab {
            Tab::Game => ("World Name", cx - 103.0),
            Tab::World => ("Seed for the world generator", cx - 153.0),
        };
        ui.text(label.1, top, label.0, GREY);
    }

    fn draw_custom(&self, _: &H, ui: &mut Ui, id: Id, [x, y, w, h]: Rect, look: Look) {
        let Id::Tab(tab) = id else { return };
        let selected = tab == self.tab;
        // Java's tab sprites: the selected tab joins the content below it.
        let fill = if selected { [0.3, 0.3, 0.3, 0.9] } else { [0.12, 0.12, 0.12, 0.9] };
        ui.rect(x, y + 2.0, w, h - 2.0, [0.0, 0.0, 0.0, 1.0]);
        ui.rect(x + 1.0, y + 3.0, w - 2.0, h - if selected { 3.0 } else { 5.0 }, fill);
        if look.focused {
            gui::focus_frame(ui, [x, y + 2.0, w, h - 2.0]);
        }
        let color = if selected && !look.hot() {
            WHITE
        } else if look.hot() {
            HOT_TEXT
        } else {
            text_color(look)
        };
        label_text(ui, [x, y + 2.0, w, h - 2.0], tab.title(), 4.0, color);
        if selected {
            // Java underlines the selected tab's title.
            let tw = Ui::text_width(tab.title());
            ui.rect((x + (w - tw) / 2.0).floor(), y + h - 6.0, tw, 1.0, color);
        }
    }
}
