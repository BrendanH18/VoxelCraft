//! v1.0 final F6: Edit Game Rules, Java 26.3's `EditGameRulesScreen` as
//! Create New World's More tab opens it (`WorldCreationGameRulesScreen`).
//! The rules sit under Java's categories, sorted as Java sorts them (by the
//! category's id, then the rule's), each with Java's name, an ON/OFF button
//! or a number box at the right, and a tooltip of its id, Java's
//! description and its default. A number out of the rule's range turns the
//! box red and greys Done. Done keeps the changes for the world being
//! created; Cancel drops them.
//!
//! Only the rules VoxelCraft enforces are listed (`GameRules::names`);
//! the others would change nothing. Java's search box is left out.

use std::cell::RefCell;
use std::rc::Rc;

use crate::app::gui::{OptionList, Rect, Screen, Size, Step, TextField, Widget, on_off};
use crate::app::options::OptionsHost;
use voxelcraft::rules::{GameRules, RuleValue};

/// Java's `GameRuleCategory`, in the order of its ids.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(in crate::app) enum Category {
    Chat,
    Drops,
    Misc,
    Mobs,
    Player,
    Spawning,
    Updates,
}

impl Category {
    /// `gamerule.category.*`.
    fn label(self) -> &'static str {
        match self {
            Category::Chat => "Chat",
            Category::Drops => "Drops",
            Category::Misc => "Miscellaneous",
            Category::Mobs => "Mobs",
            Category::Player => "Player",
            Category::Spawning => "Spawning",
            Category::Updates => "World Updates",
        }
    }
}

/// Every Java 26.3 rule's id, category, name and description
/// (`gamerule.minecraft.<id>` through `deprecated.json`'s renames, from
/// the pinned client jar's `en_us.json`), sorted by id.
pub(in crate::app) const RULES: [(&str, Category, &str, Option<&str>); 59] = [
    ("advance_time", Category::Updates, "Advance time of day", None),
    ("advance_weather", Category::Updates, "Update weather", None),
    (
        "allow_entering_nether_using_portals",
        Category::Misc,
        "Allow Nether",
        Some("Controls whether players are allowed to enter the Nether."),
    ),
    (
        "block_drops",
        Category::Drops,
        "Drop blocks",
        Some("Controls resource drops from blocks, including experience orbs."),
    ),
    (
        "block_explosion_drop_decay",
        Category::Drops,
        "In block interaction explosions, some blocks won't drop their loot",
        Some(
            "Some of the drops from blocks destroyed by explosions caused by block interactions are lost in the explosion.",
        ),
    ),
    ("command_block_output", Category::Chat, "Broadcast command block output", None),
    ("command_blocks_work", Category::Misc, "Enable Command Blocks", None),
    ("drowning_damage", Category::Player, "Deal drowning damage", None),
    ("elytra_movement_check", Category::Player, "Do elytra movement check", None),
    (
        "ender_pearls_vanish_on_death",
        Category::Player,
        "Thrown Ender Pearls vanish on death",
        Some("Whether Ender Pearls thrown by a player vanish when that player dies."),
    ),
    (
        "entity_drops",
        Category::Drops,
        "Drop entity equipment",
        Some("Controls drops from Minecarts (including inventories), Item Frames, Boats, etc."),
    ),
    ("fall_damage", Category::Player, "Deal fall damage", None),
    ("fire_damage", Category::Player, "Deal fire damage", None),
    (
        "fire_spread_radius_around_player",
        Category::Updates,
        "Fire spread radius",
        Some("The radius in blocks around a player in which fire can spread."),
    ),
    (
        "forgive_dead_players",
        Category::Mobs,
        "Forgive dead players",
        Some("Angered neutral mobs stop being angry when the targeted player dies nearby."),
    ),
    ("freeze_damage", Category::Player, "Deal freeze damage", None),
    (
        "global_sound_events",
        Category::Misc,
        "Global sound events",
        Some("When certain game events happen, like a boss spawning, the sound is heard everywhere."),
    ),
    ("immediate_respawn", Category::Player, "Respawn immediately", None),
    ("keep_inventory", Category::Player, "Keep inventory after death", None),
    (
        "lava_source_conversion",
        Category::Updates,
        "Lava converts to source",
        Some("When flowing lava is surrounded on two sides by lava sources, it converts into a source."),
    ),
    (
        "limited_crafting",
        Category::Player,
        "Require recipe for crafting",
        Some("If enabled, players will be able to craft only unlocked recipes."),
    ),
    (
        "locator_bar",
        Category::Player,
        "Enable player Locator Bar",
        Some("When enabled, a bar is shown on the screen to indicate the direction of players."),
    ),
    ("log_admin_commands", Category::Chat, "Broadcast admin commands", None),
    (
        "max_block_modifications",
        Category::Misc,
        "Command modification block limit",
        Some("The number of blocks that can be changed at once by one command, such as fill or clone."),
    ),
    (
        "max_command_forks",
        Category::Misc,
        "Command context limit",
        Some("Maximum number of contexts that can be used by commands like 'execute as'."),
    ),
    (
        "max_command_sequence_length",
        Category::Misc,
        "Command chain size limit",
        Some("Applies to command block chains and functions."),
    ),
    ("max_entity_cramming", Category::Mobs, "Entity cramming threshold", None),
    (
        "max_minecart_speed",
        Category::Misc,
        "Minecart max speed",
        Some("Maximum default speed of a moving Minecart on land."),
    ),
    (
        "max_snow_accumulation_height",
        Category::Updates,
        "Snow accumulation height",
        Some("When it snows, layers of snow form on the ground up to at most this number of layers."),
    ),
    (
        "mob_drops",
        Category::Drops,
        "Drop mob loot",
        Some("Controls resource drops from mobs, including experience orbs."),
    ),
    (
        "mob_explosion_drop_decay",
        Category::Drops,
        "In mob explosions, some blocks won't drop their loot",
        Some("Some of the drops from blocks destroyed by explosions caused by mobs are lost in the explosion."),
    ),
    ("mob_griefing", Category::Mobs, "Allow destructive mob actions", None),
    ("natural_health_regeneration", Category::Player, "Regenerate health", None),
    ("player_movement_check", Category::Player, "Do player movement check", None),
    (
        "players_nether_portal_creative_delay",
        Category::Player,
        "Player's Nether Portal delay in creative mode",
        Some(
            "Time (in ticks) that a creative mode player needs to stand in a Nether Portal before changing dimensions.",
        ),
    ),
    (
        "players_nether_portal_default_delay",
        Category::Player,
        "Player's Nether Portal delay in non-creative mode",
        Some(
            "Time (in ticks) that a non-creative mode player needs to stand in a Nether Portal before changing dimensions.",
        ),
    ),
    (
        "players_sleeping_percentage",
        Category::Player,
        "Sleep percentage",
        Some("The percentage of players who must be sleeping to skip the night."),
    ),
    (
        "projectiles_can_break_blocks",
        Category::Drops,
        "Projectiles can break blocks",
        Some("Controls whether impact projectiles will destroy blocks that are destructible by them."),
    ),
    ("pvp", Category::Player, "Enable pvp", Some("Controls whether players are allowed to damage other players.")),
    ("raids", Category::Mobs, "Do raids", None),
    ("random_tick_speed", Category::Updates, "Random tick speed rate", None),
    ("reduced_debug_info", Category::Misc, "Reduce debug info", Some("Limits contents of debug screen.")),
    (
        "respawn_radius",
        Category::Player,
        "Respawn location radius",
        Some("Controls the size of the area around the spawn point that players can spawn in."),
    ),
    ("send_command_feedback", Category::Chat, "Send command feedback", None),
    ("show_advancement_messages", Category::Chat, "Announce advancements", None),
    ("show_death_messages", Category::Chat, "Show death messages", None),
    ("spawn_mobs", Category::Spawning, "Spawn mobs", Some("Some entities might have separate rules.")),
    ("spawn_monsters", Category::Spawning, "Spawn Monsters", Some("Controls whether monsters naturally spawn.")),
    ("spawn_patrols", Category::Spawning, "Spawn pillager patrols", None),
    ("spawn_phantoms", Category::Spawning, "Spawn phantoms", None),
    ("spawn_wandering_traders", Category::Spawning, "Spawn Wandering Traders", None),
    ("spawn_wardens", Category::Spawning, "Spawn Wardens", None),
    ("spawner_blocks_work", Category::Misc, "Enable Spawner Blocks", None),
    ("spectators_generate_chunks", Category::Player, "Allow spectators to generate terrain", None),
    (
        "spread_vines",
        Category::Updates,
        "Vines spread",
        Some(
            "Controls whether the Vines block spreads randomly to adjacent blocks. Does not affect other types of vine blocks such as Weeping Vines, Twisting Vines, etc.",
        ),
    ),
    ("tnt_explodes", Category::Misc, "Allow TNT to be activated and to explode", None),
    (
        "tnt_explosion_drop_decay",
        Category::Drops,
        "In TNT explosions, some blocks won't drop their loot",
        Some("Some of the drops from blocks destroyed by explosions caused by TNT are lost in the explosion."),
    ),
    (
        "universal_anger",
        Category::Mobs,
        "Universal anger",
        Some(
            "Angered neutral mobs attack any nearby player, not just the player that angered them. Works best if forgive_dead_players is disabled.",
        ),
    ),
    (
        "water_source_conversion",
        Category::Updates,
        "Water converts to source",
        Some("When flowing water is surrounded on two sides by water sources, it converts into a source."),
    ),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Category(Category),
    Name(usize),
    Bool(usize),
    Int(usize),
    Done,
    Cancel,
}

pub(in crate::app) struct EditGameRules {
    /// The create screen's rules, written on Done.
    target: Rc<RefCell<GameRules>>,
    rules: GameRules,
    /// A number box for each integer rule, by its index in [`RULES`].
    fields: Vec<(usize, TextField)>,
}

/// The rules shown, by index in [`RULES`], in Java's order.
fn shown() -> Vec<usize> {
    let mut v: Vec<usize> = (0..RULES.len()).filter(|&i| GameRules::definition(RULES[i].0).is_some()).collect();
    v.sort_by_key(|&i| (RULES[i].1, RULES[i].0));
    v
}

impl EditGameRules {
    pub fn new(target: Rc<RefCell<GameRules>>) -> Self {
        let rules = target.borrow().clone();
        let fields = shown()
            .into_iter()
            .filter_map(|i| match rules.get(RULES[i].0) {
                Some(RuleValue::Int(v)) => Some((i, TextField::with_text(&v.to_string(), 10))),
                _ => None,
            })
            .collect();
        Self { target, rules, fields }
    }

    /// Whether a number box holds a value its rule accepts.
    fn valid(&self, i: usize) -> bool {
        self.fields
            .iter()
            .find(|(j, _)| *j == i)
            .is_none_or(|(_, f)| self.rules.clone().set(RULES[i].0, f.text()).is_ok())
    }

    fn all_valid(&self) -> bool {
        self.fields.iter().all(|(i, _)| self.valid(*i))
    }
}

/// Java's tooltip: the id, the description, then "Default: x".
fn tooltip(i: usize) -> String {
    let (id, _, _, description) = RULES[i];
    let default = GameRules::definition(id).map_or(String::new(), |d| d.default_value().to_string());
    match description {
        Some(d) => format!("{id}\n{d}\nDefault: {default}"),
        None => format!("{id}\nDefault: {default}"),
    }
}

/// Java's rule controls are 44 pixels wide at the right of the row.
const CONTROL_W: f32 = 44.0;

impl<H: OptionsHost> Screen<H> for EditGameRules {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Edit Game Rules".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<Id>> {
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        let mut category = None;
        for i in shown() {
            let (id, cat, name, _) = RULES[i];
            if category != Some(cat) {
                category = Some(cat);
                list.wide(Widget::label(Id::Category(cat), z, cat.label()));
            }
            let control = match self.rules.get(id) {
                Some(RuleValue::Bool(v)) => Widget::button(Id::Bool(i), z, on_off(v)),
                _ => Widget::field(Id::Int(i), z),
            };
            list.pair(Widget::label(Id::Name(i), z, name), Some(control.tooltip(tooltip(i))));
        }
        let done = Widget::button(Id::Done, z, "Done").enabled(self.all_valid());
        let mut widgets = list.footer_pair(done, Widget::button(Id::Cancel, z, "Cancel"), size);
        // The control at the row's right end; the name takes the rest.
        for w in &mut widgets {
            if let Id::Bool(_) | Id::Int(_) = w.id {
                let [x, y, cw, h] = w.rect;
                w.rect = [x + cw - CONTROL_W, y, CONTROL_W, h];
            } else if let Id::Name(_) = w.id {
                w.rect[2] = crate::app::gui::LIST_W - CONTROL_W - 4.0;
            }
        }
        widgets
    }

    fn activate(&mut self, _: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Bool(i) => {
                if let Some(RuleValue::Bool(v)) = self.rules.get(RULES[i].0) {
                    let _ = self.rules.set(RULES[i].0, if v { "false" } else { "true" });
                }
            }
            Id::Done if self.all_valid() => {
                for (i, f) in &self.fields {
                    let _ = self.rules.set(RULES[*i].0, f.text());
                }
                *self.target.borrow_mut() = self.rules.clone();
                return Step::Pop;
            }
            Id::Cancel => return Step::Pop,
            _ => {}
        }
        Step::Stay
    }

    fn back(&mut self, _: &mut H) -> Step<H> {
        Step::Pop
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        let Id::Int(i) = id else { return None };
        self.fields.iter_mut().find(|(j, _)| *j == i).map(|(_, f)| f)
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        let Id::Int(i) = id else { return None };
        self.fields.iter().find(|(j, _)| *j == i).map(|(_, f)| f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every listed rule is Java's, under its category, in Java's order,
    /// and Done writes the edits back while Cancel drops them.
    #[test]
    fn edit_game_rules_lists_javas_rules_and_keeps_done_edits() {
        for (id, ..) in RULES {
            assert!(voxelcraft::rules::JAVA_RULES.iter().any(|r| r.id == id), "{id} is a Java 26.3 rule");
        }
        let order = shown();
        assert!(order.windows(2).all(|w| (RULES[w[0]].1, RULES[w[0]].0) < (RULES[w[1]].1, RULES[w[1]].0)));
        assert_eq!(order.len(), GameRules::names().count(), "every enforced rule is listed");
        let target = Rc::new(RefCell::new(GameRules::default()));
        let keep = RULES.iter().position(|r| r.0 == "keep_inventory").unwrap();
        let tick = RULES.iter().position(|r| r.0 == "random_tick_speed").unwrap();
        assert_eq!(RULES[keep].2, "Keep inventory after death");
        type Host = crate::app::options::tests::Host;
        let mut host = Host::default();
        let mut screen = EditGameRules::new(Rc::clone(&target));
        screen.activate(&mut host, Id::Bool(keep));
        Screen::<Host>::field(&mut screen, Id::Int(tick)).unwrap().set("-1");
        assert!(!screen.all_valid(), "out of range");
        Screen::<Host>::field(&mut screen, Id::Int(tick)).unwrap().set("10");
        assert!(matches!(screen.activate(&mut host, Id::Done), Step::Pop));
        assert!(target.borrow().bool("keep_inventory"));
        assert_eq!(target.borrow().int("random_tick_speed"), 10);
        let mut screen = EditGameRules::new(Rc::clone(&target));
        screen.activate(&mut host, Id::Bool(keep));
        screen.activate(&mut host, Id::Cancel);
        assert!(target.borrow().bool("keep_inventory"), "Cancel drops the change");
    }
}
