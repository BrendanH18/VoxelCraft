//! v1.0 gap Track 8g: death messages as Java words them
//! (`DamageSource.getLocalizedDeathMessage`, `CombatTracker.getDeathMessage`,
//! the `death.attack.*` and `death.fell.*` texts of `en_us.json`).
//!
//! A [`Cause`]'s text names its attacker VoxelCraft's way ("was slain by a
//! zombie"); [`java_words`] puts in the entity's Java display name
//! ("was slain by Zombie", `entity.minecraft.zombie`). A player's death
//! keeps a [`CombatTracker`] of the hits they took, which picks Java's
//! message: a fall's `death.fell.assist`/`finish` naming who knocked them
//! off, a `.player` variant naming who they were escaping (`getKillCredit`)
//! when nothing else did, and the message's [`Pending`] attacker, whose
//! custom name and named weapon (`.item`) the game puts in once it can look
//! them up ([`Pending::finish`]).
//!
//! `tests/java_damage_types.rs` checks the variants against the fixture's
//! `death_messages`.

use std::ops::Range;
use std::sync::OnceLock;

use super::damage::{Cause, DamageTag, DamageType, DeathMessageType, PLAYER};
use crate::entity::trident::Owner;

/// `LivingEntity.lastHurtByPlayerMemoryTime` and the 100 ticks
/// `baseTick` keeps `lastHurtByMob`, in seconds.
pub const CREDIT_MEMORY: f32 = 5.0;
/// `CombatTracker.RESET_DAMAGE_STATUS_TIME` and `RESET_COMBAT_STATUS_TIME`,
/// in seconds: the hits are forgotten this long after the last one.
pub const RESET_DAMAGE: f32 = 5.0;
pub const RESET_COMBAT: f32 = 15.0;

/// Java's `entity.minecraft.*` name for a mob kind: its words capitalised,
/// "Zombified Piglin", "Polar Bear" (every kind VoxelCraft has is named so).
pub fn mob_name(kind: crate::entity::MobKind) -> String {
    title(kind.name())
}

fn title(name: &str) -> String {
    name.split([' ', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every entity name a cause's text can hold (lowercase, spaced) with its
/// Java display name: the mob kinds, the dragon, and the projectiles an
/// ownerless hit names itself by.
fn names() -> &'static [(String, String)] {
    static NAMES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut v: Vec<(String, String)> = crate::entity::MobKind::ALL
            .iter()
            .map(|&k| (k.name().replace('_', " ").to_ascii_lowercase(), mob_name(k)))
            .collect();
        for (name, java) in [
            ("ender dragon", "Ender Dragon"),
            ("arrow", "Arrow"),
            ("wind charge", "Wind Charge"),
            ("splash potion", "Splash Potion"),
            ("lingering potion", "Lingering Potion"),
            ("area effect cloud", "Area Effect Cloud"),
        ] {
            v.push((name.into(), java.into()));
        }
        v
    })
}

/// A death message (after the victim's name) and where the name of the
/// entity it names stands in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worded {
    pub text: String,
    pub name: Option<Range<usize>>,
}

/// `text` (a [`Cause`]'s) with the entity it names ("a zombie", "an
/// evoker", "the Ender Dragon") put as Java names it ("Zombie"), and where
/// that name stands. A text ending in [`PLAYER`] keeps it, standing for the
/// player until the game names them. A text already in Java's words comes
/// back unchanged (with no name found).
pub fn java_words(text: &str) -> Worded {
    if let Some(head) = text.strip_suffix(PLAYER) {
        return Worded { text: text.to_string(), name: Some(head.len()..text.len()) };
    }
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut best: Option<(usize, usize, &str)> = None;
    for (name, java) in names() {
        for article in ["a ", "an ", "the "] {
            let phrase = format!("{article}{name}");
            let mut from = 0;
            while let Some(i) = lower[from..].find(&phrase) {
                let start = from + i;
                let end = start + phrase.len();
                let bounded = (start == 0 || bytes[start - 1] == b' ') && (end == bytes.len() || bytes[end] == b' ');
                if bounded && best.is_none_or(|(_, len, _)| phrase.len() > len) {
                    best = Some((start, phrase.len(), java));
                }
                from = start + 1;
            }
        }
    }
    match best {
        Some((start, len, java)) => {
            let worded = format!("{}{java}{}", &text[..start], &text[start + len..]);
            Worded { text: worded, name: Some(start..start + java.len()) }
        }
        None => Worded { text: text.to_string(), name: None },
    }
}

/// `death.attack.<id>.player` (the kill credit's variant) and
/// `death.attack.<id>.item` (a named weapon's) after `"%1$s "`, from
/// Java 26.3's `en_us.json`; `%2$s` is the attacker, `%3$s` the weapon.
pub const VARIANTS: &[(&str, &str)] = &[
    ("death.attack.anvil.player", "was squashed by a falling anvil while fighting %2$s"),
    ("death.attack.arrow.item", "was shot by %2$s using %3$s"),
    ("death.attack.cactus.player", "walked into a cactus while trying to escape %2$s"),
    ("death.attack.cramming.player", "was squashed by %2$s"),
    ("death.attack.dragonBreath.player", "was roasted in dragon's breath by %2$s"),
    ("death.attack.drown.player", "drowned while trying to escape %2$s"),
    ("death.attack.dryout.player", "died from dehydration while trying to escape %2$s"),
    ("death.attack.explosion.player", "was blown up by %2$s"),
    ("death.attack.explosion.player.item", "was blown up by %2$s using %3$s"),
    ("death.attack.fall.player", "hit the ground too hard while trying to escape %2$s"),
    ("death.attack.fallingBlock.player", "was squashed by a falling block while fighting %2$s"),
    ("death.attack.fallingStalactite.player", "was skewered by a falling stalactite while fighting %2$s"),
    ("death.attack.fireball.item", "was fireballed by %2$s using %3$s"),
    ("death.attack.fireworks.item", "went off with a bang due to a firework fired from %3$s by %2$s"),
    ("death.attack.fireworks.player", "went off with a bang while fighting %2$s"),
    ("death.attack.flyIntoWall.player", "experienced kinetic energy while trying to escape %2$s"),
    ("death.attack.freeze.player", "was frozen to death by %2$s"),
    ("death.attack.generic.player", "died because of %2$s"),
    ("death.attack.genericKill.player", "was killed while fighting %2$s"),
    ("death.attack.hotFloor.player", "walked into the danger zone due to %2$s"),
    ("death.attack.inFire.player", "walked into fire while fighting %2$s"),
    ("death.attack.inWall.player", "suffocated in a wall while fighting %2$s"),
    ("death.attack.indirectMagic.item", "was killed by %2$s using %3$s"),
    ("death.attack.lava.player", "tried to swim in lava to escape %2$s"),
    ("death.attack.lightningBolt.player", "was struck by lightning while fighting %2$s"),
    ("death.attack.mace_smash.item", "was smashed by %2$s with %3$s"),
    ("death.attack.magic.player", "was killed by magic while trying to escape %2$s"),
    ("death.attack.mob.item", "was slain by %2$s using %3$s"),
    ("death.attack.onFire.item", "was burned to a crisp while fighting %2$s wielding %3$s"),
    ("death.attack.onFire.player", "was burned to a crisp while fighting %2$s"),
    ("death.attack.outOfWorld.player", "didn't want to live in the same world as %2$s"),
    ("death.attack.outsideBorder.player", "left the confines of this world while fighting %2$s"),
    ("death.attack.player.item", "was slain by %2$s using %3$s"),
    ("death.attack.sonic_boom.item", "was obliterated by a sonically-charged shriek while trying to escape %2$s wielding %3$s"),
    ("death.attack.sonic_boom.player", "was obliterated by a sonically-charged shriek while trying to escape %2$s"),
    ("death.attack.spear.item", "was speared by %2$s using %3$s"),
    ("death.attack.stalagmite.player", "was impaled on a stalagmite while fighting %2$s"),
    ("death.attack.starve.player", "starved to death while fighting %2$s"),
    ("death.attack.sting.item", "was stung to death by %2$s using %3$s"),
    ("death.attack.sting.player", "was stung to death by %2$s"),
    ("death.attack.sweetBerryBush.player", "was poked to death by a sweet berry bush while trying to escape %2$s"),
    ("death.attack.thorns.item", "was killed by %3$s while trying to hurt %2$s"),
    ("death.attack.thrown.item", "was pummeled by %2$s using %3$s"),
    ("death.attack.trident.item", "was impaled by %2$s with %3$s"),
    ("death.attack.wither.player", "withered away while fighting %2$s"),
    ("death.attack.witherSkull.item", "was shot by a skull from %2$s using %3$s"),
    ("death.fell.accident.generic", "fell from a high place"),
    ("death.fell.assist", "was doomed to fall by %2$s"),
    ("death.fell.assist.item", "was doomed to fall by %2$s using %3$s"),
    ("death.fell.finish", "fell too far and was finished by %2$s"),
    ("death.fell.finish.item", "fell too far and was finished by %2$s using %3$s"),
    ("death.fell.killer", "was doomed to fall"),
];

fn variant(key: &str) -> Option<&'static str> {
    VARIANTS.iter().find(|(k, _)| *k == key).map(|&(_, v)| v)
}

/// `"death.attack." + msgId + suffix`.
fn attack_variant(kind: DamageType, suffix: &str) -> Option<&'static str> {
    variant(&format!("death.attack.{}{suffix}", kind.message_id()))
}

/// `template` with the attacker's and weapon's names put in.
fn fill(template: &str, name: &str, weapon: Option<&str>) -> String {
    let text = template.replace("%2$s", name);
    match weapon {
        Some(w) => text.replace("%3$s", &format!("[{w}]")),
        None => text,
    }
}

/// Whether Java's damage source for `kind` has no entity: the fixed sources
/// `DamageSources` builds with `source(type)` alone, which name whoever the
/// victim was escaping (`getKillCredit`) in their `.player` variant.
pub const fn entityless(kind: DamageType) -> bool {
    use DamageType as T;
    matches!(
        kind,
        T::InFire
            | T::Campfire
            | T::LightningBolt
            | T::OnFire
            | T::Lava
            | T::HotFloor
            | T::InWall
            | T::Cramming
            | T::Drown
            | T::Starve
            | T::Cactus
            | T::Fall
            | T::EnderPearl
            | T::FlyIntoWall
            | T::OutOfWorld
            | T::Generic
            | T::Magic
            | T::Wither
            | T::DragonBreath
            | T::DryOut
            | T::SweetBerryBush
            | T::Freeze
            | T::Stalagmite
            | T::OutsideBorder
            | T::GenericKill
    )
}

/// Who an attacker is, as the game looks them up when the message is
/// finished: their display name (a player's profile name, a mob's custom
/// name or its kind's Java name) and their main-hand item's custom name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attacker {
    pub name: String,
    pub weapon: Option<String>,
    /// Still alive: a kill credit names only a living mob (`baseTick`
    /// forgets a dead `lastHurtByMob`); a message naming its killer names
    /// them dead or alive.
    pub alive: bool,
}

/// What a death message still needs from the game: the entity it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub owner: Owner,
    pub shape: Shape,
}

/// How [`Pending::finish`] puts the attacker in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The message names them where `name` stands (their kind's name, or
    /// "a player"); a named weapon turns it into `item` (the `.item`
    /// variant) when there is one. If they can't be found, `fallback`
    /// replaces the message when there is one.
    Named { name: Range<usize>, item: Option<&'static str>, fallback: Option<&'static str> },
    /// The message names no one; if they're still about, Java's `template`
    /// (the `.player` kill credit variant) names them instead.
    Credit { template: &'static str },
}

impl Pending {
    /// `death` finished with the attacker `lookup` finds (None: gone, or a
    /// kill credit's mob no longer alive), as Java names them.
    pub fn finish(&self, death: &mut String, lookup: impl FnOnce(Owner) -> Option<Attacker>) {
        let Some(attacker) = lookup(self.owner) else {
            if let Shape::Named { fallback: Some(fallback), .. } = self.shape {
                *death = fallback.to_string();
            }
            return;
        };
        match &self.shape {
            Shape::Named { name, item, .. } => {
                if let (Some(item), Some(weapon)) = (item, attacker.weapon.as_deref()) {
                    *death = fill(item, &attacker.name, Some(weapon));
                } else if !attacker.name.is_empty() && death.get(name.clone()).is_some() {
                    death.replace_range(name.clone(), &attacker.name);
                }
            }
            Shape::Credit { template } if attacker.alive => *death = fill(template, &attacker.name, None),
            Shape::Credit { .. } => {}
        }
    }
}

/// One hit in a [`CombatTracker`] (`CombatEntry`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    by: Option<Owner>,
    /// `#is_fall` or `#always_most_significant_fall`.
    fall: bool,
    always_most_significant: bool,
    fall_distance: f32,
}

/// `CombatTracker` for a player: the hits that took health, until a quiet
/// spell (or death) clears them. Fall locations aren't kept: every fall is
/// Java's `GENERIC`, and ladders, vines and water don't pick a hit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CombatTracker {
    entries: Vec<Entry>,
    /// Seconds since the last recorded hit.
    since: f32,
    in_combat: bool,
}

/// At most this many hits are kept: a long fight's early hits only matter
/// as the hit before the biggest fall.
const MAX_ENTRIES: usize = 64;

impl CombatTracker {
    /// `recheckStatus` after `dt` seconds.
    pub fn tick(&mut self, dt: f32, alive: bool) {
        self.since = (self.since + dt).min(1e3);
        let reset = if self.in_combat { RESET_COMBAT } else { RESET_DAMAGE };
        if !self.entries.is_empty() && (!alive || self.since > reset) {
            self.entries.clear();
            self.in_combat = false;
        }
    }

    /// `recordDamage`: a hit of `cause` that took health, the player
    /// `fall_distance` blocks into a fall.
    pub fn record(&mut self, cause: Cause, fall_distance: f32) {
        let by = attacker(cause);
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(Entry {
            by,
            fall: cause.is(DamageTag::IsFall),
            always_most_significant: cause.is(DamageTag::AlwaysMostSignificantFall),
            fall_distance,
        });
        self.since = 0.0;
        if by.is_some() {
            self.in_combat = true;
        }
    }

    /// `getMostSignificantFall`: the hit before the biggest fall over five
    /// blocks (the fall itself when it came first). Without fall locations
    /// Java's alternative (the hardest hit while climbing) never applies.
    fn knock_off(&self) -> Option<Entry> {
        let mut result = None;
        let mut best = 0.0f32;
        for (i, e) in self.entries.iter().enumerate() {
            let distance = if e.always_most_significant { f32::MAX } else { e.fall_distance };
            if (e.fall || e.always_most_significant) && distance > 0.0 && (result.is_none() || distance > best) {
                result = Some(if i > 0 { self.entries[i - 1] } else { *e });
                best = distance;
            }
        }
        result.filter(|_| best > 5.0)
    }

    /// `getDeathMessage` for the killing blow `cause` (already recorded):
    /// the message, Java-worded, and whom it still needs named. `credit` is
    /// the victim's kill credit (`getKillCredit`).
    pub fn death_message(&self, cause: Cause, credit: Option<Owner>) -> (String, Option<Pending>) {
        let by = attacker(cause);
        if cause.kind.death_message_type() == DeathMessageType::FallVariants
            && let Some(knock) = self.knock_off()
        {
            if knock.fall || knock.always_most_significant {
                return (variant("death.fell.accident.generic").unwrap_or(cause.text).to_string(), None);
            }
            let (owner, key) = match (knock.by, by) {
                (Some(a), killer) if Some(a) != killer => (a, "death.fell.assist"),
                (_, Some(killer)) => (killer, "death.fell.finish"),
                _ => return (variant("death.fell.killer").unwrap_or(cause.text).to_string(), None),
            };
            let template = variant(key).unwrap_or(cause.text);
            let text = template.replace("%2$s", PLAYER);
            let at = template.find("%2$s").unwrap_or(0);
            let item = variant(&format!("{key}.item"));
            // Gone before the message was put together: no one to name.
            let fallback = variant("death.fell.killer");
            return (text, Some(Pending { owner, shape: Shape::Named { name: at..at + PLAYER.len(), item, fallback } }));
        }
        // A fall under six blocks: Java's `death.attack.fall`, through
        // `getLocalizedDeathMessage`.
        let text = if cause.kind.death_message_type() == DeathMessageType::FallVariants && entityless(cause.kind) {
            "hit the ground too hard"
        } else {
            cause.text
        };
        let worded = java_words(text);
        if cause.kind.death_message_type() == DeathMessageType::IntentionalGameDesign {
            return (worded.text, None);
        }
        if let Some(owner) = by {
            let pending = worded.name.clone().map(|name| Pending {
                owner,
                shape: Shape::Named { name, item: attack_variant(cause.kind, ".item"), fallback: None },
            });
            return (worded.text, pending);
        }
        if entityless(cause.kind)
            && worded.name.is_none()
            && let (Some(owner), Some(template)) = (credit, attack_variant(cause.kind, ".player"))
        {
            return (worded.text, Some(Pending { owner, shape: Shape::Credit { template } }));
        }
        (worded.text, None)
    }
}

/// The living entity a cause names (`DamageSource.getEntity`, or the player
/// a crystal's blast names).
pub fn attacker(cause: Cause) -> Option<Owner> {
    cause.names.map(Owner::Player).or(cause.by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PlayerId;

    #[test]
    fn mobs_are_named_as_java_names_them() {
        assert_eq!(java_words("was slain by a zombie").text, "was slain by Zombie");
        assert_eq!(java_words("was slain by a zombie villager").text, "was slain by Zombie Villager");
        assert_eq!(java_words("was killed by an evoker using magic").text, "was killed by Evoker using magic");
        assert_eq!(java_words("was slain by the Ender Dragon").text, "was slain by Ender Dragon");
        assert_eq!(java_words("was blown up by the Wither").text, "was blown up by Wither");
        assert_eq!(java_words("was slain by a polar bear").text, "was slain by Polar Bear");
        assert_eq!(java_words("was shot by an arrow").text, "was shot by Arrow");
        let w = java_words("was slain by an iron golem");
        assert_eq!(&w.text[w.name.unwrap()], "Iron Golem");
        assert_eq!(java_words("drowned"), Worded { text: "drowned".into(), name: None });
        assert_eq!(java_words("was slain by Zombie").name, None, "Java's words stay");
        let p = java_words("was slain by a player");
        assert_eq!(&p.text[p.name.unwrap()], PLAYER);
        assert_eq!(mob_name(crate::entity::MobKind::ZombifiedPiglin), "Zombified Piglin");
        assert_eq!(mob_name(crate::entity::MobKind::PolarBear), "Polar Bear");
    }

    fn named(name: &str, weapon: Option<&str>) -> Option<Attacker> {
        Some(Attacker { name: name.into(), weapon: weapon.map(Into::into), alive: true })
    }

    #[test]
    fn custom_names_and_named_weapons_finish_the_message() {
        let t = CombatTracker::default();
        let zombie = Cause::mob("was slain by a zombie").by(Some(Owner::Mob(7)));
        let (mut text, pending) = t.death_message(zombie, None);
        assert_eq!(text, "was slain by Zombie");
        pending.clone().unwrap().finish(&mut text, |o| {
            assert_eq!(o, Owner::Mob(7));
            named("Grumbles", None)
        });
        assert_eq!(text, "was slain by Grumbles");
        let (mut text, pending) = t.death_message(zombie, None);
        pending.unwrap().finish(&mut text, |_| named("Zombie", Some("Biter")));
        assert_eq!(text, "was slain by Zombie using [Biter]");
        let alex = Cause::PLAYER_TRIDENT.by(Some(Owner::Player(PlayerId(2))));
        let (mut text, pending) = t.death_message(alex, None);
        pending.unwrap().finish(&mut text, |_| named("Alex", Some("Poseidon")));
        assert_eq!(text, "was impaled by Alex with [Poseidon]");
        // Gone: the kind's name stays.
        let (mut text, pending) = t.death_message(zombie, None);
        pending.unwrap().finish(&mut text, |_| None);
        assert_eq!(text, "was slain by Zombie");
    }

    #[test]
    fn entityless_deaths_name_who_the_player_was_escaping() {
        let t = CombatTracker::default();
        let alex = Some(Owner::Player(PlayerId(2)));
        let (mut text, pending) = t.death_message(Cause::DROWN, alex);
        assert_eq!(text, "drowned");
        pending.unwrap().finish(&mut text, |_| named("Alex", Some("Sword")));
        assert_eq!(text, "drowned while trying to escape Alex", "no .item for the credit");
        let (mut text, pending) = t.death_message(Cause::LAVA, Some(Owner::Mob(3)));
        pending.unwrap().finish(&mut text, |_| named("Zombie", None));
        assert_eq!(text, "tried to swim in lava to escape Zombie");
        let (mut text, pending) = t.death_message(Cause::MAGIC, alex);
        pending.unwrap().finish(&mut text, |_| named("Alex", None));
        assert_eq!(text, "was killed by magic while trying to escape Alex");
        // A dead mob gives no credit.
        let (mut text, pending) = t.death_message(Cause::ON_FIRE, Some(Owner::Mob(3)));
        pending.unwrap().finish(&mut text, |_| Some(Attacker { name: "Zombie".into(), weapon: None, alive: false }));
        assert_eq!(text, super::super::survival::CAUSE_FIRE);
        // An entity's own message never takes a credit.
        assert_eq!(t.death_message(Cause::mob("was slain by a husk"), alex).1, None);
    }

    #[test]
    fn falls_name_who_knocked_the_player_off() {
        let zombie = Some(Owner::Mob(4));
        let mut t = CombatTracker::default();
        t.record(Cause::mob("was slain by a zombie").by(zombie), 0.0);
        t.record(Cause::FALL, 12.0);
        let (mut text, pending) = t.death_message(Cause::FALL, None);
        assert_eq!(text, "was doomed to fall by a player");
        pending.unwrap().finish(&mut text, |_| named("Zombie", None));
        assert_eq!(text, "was doomed to fall by Zombie");
        let (mut text, pending) = t.death_message(Cause::FALL, None);
        pending.unwrap().finish(&mut text, |_| named("Alex", Some("Shove")));
        assert_eq!(text, "was doomed to fall by Alex using [Shove]");
        let (mut text, pending) = t.death_message(Cause::FALL, None);
        pending.unwrap().finish(&mut text, |_| None);
        assert_eq!(text, "was doomed to fall", "the zombie is gone");
        // A fall on its own.
        let mut t = CombatTracker::default();
        t.record(Cause::FALL, 12.0);
        assert_eq!(t.death_message(Cause::FALL, None), ("fell from a high place".into(), None));
        // Under six blocks: `death.attack.fall`, with the credit.
        let mut t = CombatTracker::default();
        t.record(Cause::FALL, 4.0);
        let (mut text, pending) = t.death_message(Cause::FALL, zombie);
        assert_eq!(text, "hit the ground too hard");
        pending.unwrap().finish(&mut text, |_| named("Zombie", None));
        assert_eq!(text, "hit the ground too hard while trying to escape Zombie");
        // A hit with no one behind it before the fall.
        let mut t = CombatTracker::default();
        t.record(Cause::CAMPFIRE, 0.0);
        t.record(Cause::FALL, 9.0);
        assert_eq!(t.death_message(Cause::FALL, None), ("was doomed to fall".into(), None));
        // Quiet for five seconds out of combat: forgotten.
        let mut t = CombatTracker::default();
        t.record(Cause::CAMPFIRE, 0.0);
        t.tick(5.5, true);
        t.record(Cause::FALL, 9.0);
        assert_eq!(t.death_message(Cause::FALL, None).0, "fell from a high place");
    }
}
