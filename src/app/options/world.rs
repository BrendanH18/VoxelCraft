//! v1.0 final F6: World Options, the pause menu screen Java 26.3 moved
//! Difficulty and its lock to (`WorldOptionsScreen`). Changes wait for Apply
//! Changes, which asks first when the lock changed (`difficulty.lock.title`);
//! Cancel drops them.
//!
//! VoxelCraft's World Options has Java's General section's Difficulty and
//! lock. Left out: the default and personal game modes, Allow Commands, Edit
//! Game Rules and Restrictions (VoxelCraft has no in-world game rule editor
//! or chat restrictions; `/gamerule` and `/gamemode` change them), and the
//! Multiplayer section, whose LAN controls stay on the pause menu's Open to
//! LAN screen. A LAN guest, who can't change the host's difficulty, sees no
//! difficulty here.

use super::{OptionsHost, WorldDifficulty, draw_lock};
use crate::app::gui::{Confirm, Look, OptionList, Rect, Screen, Size, Step, Widget};
use crate::render::ui::Ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    General,
    Difficulty,
    Lock,
    Apply,
    Cancel,
}

/// Java's `LockIconButton` is square, beside a narrowed difficulty button.
const LOCK_W: f32 = 20.0;

#[derive(Default)]
pub(in crate::app) struct WorldOptions {
    /// The world's difficulty when the screen opened, and what Apply Changes
    /// would make it; `None` when there is none to change.
    initial: Option<WorldDifficulty>,
    wanted: Option<WorldDifficulty>,
    /// False until the world's difficulty has been read (a screen opened by
    /// `--open-menu` before the world is at hand reads it on first use).
    loaded: bool,
}

impl WorldOptions {
    pub fn new<H: OptionsHost>(host: &H) -> Self {
        let d = host.difficulty();
        Self { initial: d, wanted: d, loaded: true }
    }

    /// The initial and wanted difficulty.
    fn state<H: OptionsHost>(&self, host: &H) -> (Option<WorldDifficulty>, Option<WorldDifficulty>) {
        if self.loaded { (self.initial, self.wanted) } else { (host.difficulty(), host.difficulty()) }
    }

    fn load<H: OptionsHost>(&mut self, host: &H) {
        if !self.loaded {
            *self = Self::new(host);
        }
    }

    fn changed(&self) -> bool {
        self.wanted != self.initial
    }
}

/// Applies `wanted` over `initial`: the difficulty, then the lock.
fn apply<H: OptionsHost>(host: &mut H, initial: WorldDifficulty, wanted: WorldDifficulty) {
    if wanted.level != initial.level {
        host.set_difficulty(wanted.level);
    }
    if wanted.locked && !initial.locked {
        host.lock_difficulty();
    }
}

impl<H: OptionsHost> Screen<H> for WorldOptions {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "World Options".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let (initial, wanted) = self.state(host);
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        list.wide(Widget::label(Id::General, z, "General"));
        if let (Some(initial), Some(wanted)) = (initial, wanted) {
            // `updateDifficultyButtonsState`: both follow the world's own lock.
            let w = Widget::button(Id::Difficulty, z, format!("Difficulty: {}", wanted.level));
            let w = if initial.hardcore {
                w.enabled(false).tooltip("Cannot change the difficulty in a hardcore world.")
            } else if initial.locked {
                w.enabled(false).tooltip("Difficulty is locked.")
            } else {
                w
            };
            list.pair(w, None);
        }
        let apply = Widget::button(Id::Apply, z, "Apply Changes").enabled(wanted != initial);
        let mut widgets = list.footer_pair(apply, Widget::button(Id::Cancel, z, "Cancel"), size);
        // The lock takes the right end of the difficulty button.
        if let (Some(initial), Some(i)) = (initial, widgets.iter().position(|w| w.id == Id::Difficulty)) {
            let [x, y, w, h] = widgets[i].rect;
            widgets[i].rect = [x, y, w - LOCK_W, h];
            let lock = Widget::custom(Id::Lock, [x + w - LOCK_W, y, LOCK_W, h]).scrolling();
            let lock = if initial.locked || initial.hardcore {
                lock.enabled(false)
            } else {
                lock.tooltip("Lock World Difficulty")
            };
            widgets.insert(i + 1, lock);
        }
        widgets
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        self.load(host);
        match id {
            Id::General => {}
            Id::Difficulty => {
                if let Some(w) = &mut self.wanted
                    && !w.hardcore
                    && !w.locked_in(self.initial)
                {
                    w.level = w.level.next();
                }
            }
            Id::Lock => {
                if let Some(w) = &mut self.wanted
                    && !w.hardcore
                    && !w.locked_in(self.initial)
                {
                    w.locked = !w.locked;
                }
            }
            Id::Apply => {
                let (Some(initial), Some(wanted)) = (self.initial, self.wanted) else { return Step::Pop };
                if !self.changed() {
                    return Step::Stay;
                }
                if wanted.locked != initial.locked {
                    let question = format!(
                        "Are you sure you want to lock the difficulty of this world? This will set this world to always be {}, and you will never be able to change that again.",
                        wanted.level
                    );
                    return Step::push(
                        Confirm::new("Lock World Difficulty", vec![question], "Yes", move |host: &mut H| {
                            apply(host, initial, wanted);
                            Step::Pop
                        })
                        .no("No"),
                    );
                }
                apply(host, initial, wanted);
                return Step::Pop;
            }
            Id::Cancel => return Step::Pop,
        }
        Step::Stay
    }

    fn back(&mut self, _: &mut H) -> Step<H> {
        Step::Pop
    }

    fn draw_custom(&self, host: &H, ui: &mut Ui, _: Id, rect: Rect, look: Look) {
        let locked = self.state(host).1.is_some_and(|d| d.locked || d.hardcore);
        draw_lock(ui, rect, locked, look);
    }
}

impl WorldDifficulty {
    /// Locked in the world itself (not just wanted): nothing can change.
    fn locked_in(&self, initial: Option<WorldDifficulty>) -> bool {
        initial.is_some_and(|d| d.locked)
    }
}
