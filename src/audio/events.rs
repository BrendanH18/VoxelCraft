//! v0.18 Track 9: the Java 26.3 sound event each VoxelCraft [`Sound`]
//! stands for, which names its subtitle (see [`super::subtitles`]).
//!
//! A sound with several Java counterparts names the event of its main use
//! (a [`Sound::Door`] is `block.wooden_door.*` even where it stands in for a
//! fence gate); the [verification record](../../docs/verification/2026-11-23-v0.18-sound.md)
//! lists those stand-ins. `None` marks VoxelCraft's own sounds with no Java
//! event: the synthesized wind and cave room tone, whose loops never pass
//! through [`super::Audio::play`].

use super::sounds::{BlockCue, ContainerCue, Material, Sound};
use super::voices::{Call, Voice};
use crate::world::gadgets::Instrument;

/// Java's entity name for a voice's events (`entity.<name>.<call>`).
fn voice_entity(voice: Voice) -> &'static str {
    match voice {
        // Cod, salmon, tropical fish, pufferfish, tadpoles and turtles share
        // one voice; cod's events stand for them.
        Voice::Fish => "cod",
        Voice::Allay => "allay",
        v => v.name(),
    }
}

impl Sound {
    /// The Java 26.3 sound event this sound plays as, or `None` for a
    /// VoxelCraft-only sound.
    pub fn java_event(self) -> Option<String> {
        let s = |e: &str| Some(e.to_string());
        match self {
            Sound::Thunder => s("entity.lightning_bolt.thunder"),
            Sound::HoneySlide => s("block.honey_block.slide"),
            Sound::HoneyDrink => s("item.honey_bottle.drink"),
            Sound::Note(i) => Some(format!("block.note_block.{}", note_event(i))),
            // Water stands in for the bucket and swimming events.
            Sound::Break(Material::Water) => s("entity.generic.splash"),
            Sound::Place(Material::Water) => s("item.bucket.empty"),
            Sound::Step(Material::Water) => s("entity.player.swim"),
            Sound::Break(m) => Some(format!("block.{}.break", material_event(m))),
            Sound::Place(m) => Some(format!("block.{}.place", material_event(m))),
            Sound::Step(m) => Some(format!("block.{}.step", material_event(m))),
            Sound::Land => s("entity.player.big_fall"),
            Sound::Splash => s("entity.player.splash"),
            Sound::Swim => s("entity.player.swim"),
            Sound::Click => s("ui.button.click"),
            Sound::Drip => s("block.pointed_dripstone.drip_water"),
            Sound::Wind | Sound::Cave => None,
            Sound::Explosion => s("entity.generic.explode"),
            Sound::Fuse => s("entity.creeper.primed"),
            Sound::Bow => s("entity.arrow.shoot"),
            Sound::Pop => s("entity.item.pickup"),
            Sound::Hurt => s("entity.player.hurt"),
            Sound::Hit => s("entity.player.attack.strong"),
            Sound::Mob(v, c) => Some(mob_event(v, c)),
            Sound::Rain => s("weather.rain"),
            Sound::Door(open) => s(if open { "block.wooden_door.open" } else { "block.wooden_door.close" }),
            Sound::Orb => s("entity.experience_orb.pickup"),
            Sound::LevelUp => s("entity.player.levelup"),
            Sound::Teleport => s("entity.enderman.teleport"),
            Sound::Scream => s("entity.enderman.stare"),
            Sound::Fireball => s("entity.blaze.shoot"),
            Sound::EyeDeath => s("entity.ender_eye.death"),
            Sound::FrameFill => s("block.end_portal_frame.fill"),
            Sound::PortalSpawn => s("block.end_portal.spawn"),
            Sound::DragonFlap => s("entity.ender_dragon.flap"),
            Sound::DragonGrowl => s("entity.ender_dragon.growl"),
            Sound::DragonDeath => s("entity.ender_dragon.death"),
            Sound::Bell => s("block.bell.use"),
            Sound::CopperGolem(a, stage) => Some(copper_golem_event(a, stage)),
            Sound::Sniffer(c) => s(sniffer_event(c)),
            Sound::Allay(a) => {
                use crate::entity::allay::Sound as A;
                s(match a {
                    A::Give => "entity.allay.item_given",
                    A::Take => "entity.allay.item_taken",
                    A::Throw => "entity.allay.item_thrown",
                    A::Duplicate => "entity.allay.ambient_with_item",
                })
            }
            Sound::Warden(c) => Some(warden_event(c)),
            Sound::Sculk(c) => s(sculk_event(c)),
            Sound::Weapon(c) => s(weapon_event(c)),
            Sound::Breeze(c) => s(breeze_event(c)),
            Sound::Illager(c) => {
                let key = c.key();
                let kind = if key.starts_with("crossbow.") { "item" } else { "entity" };
                Some(format!("{kind}.{key}"))
            }
            Sound::Raid(c) => s(c.key()),
            Sound::Flight(c) => s(c.key()),
            Sound::Wither(c) => {
                use crate::entity::wither::Cue as C;
                s(match c {
                    C::Spawn => "entity.wither.spawn",
                    C::Shoot => "entity.wither.shoot",
                    C::BreakBlocks => "entity.wither.break_block",
                })
            }
            Sound::Beacon(c) => {
                use crate::world::beacon::Cue as C;
                s(match c {
                    C::Activate => "block.beacon.activate",
                    C::Ambient => "block.beacon.ambient",
                    C::PowerSelect => "block.beacon.power_select",
                    C::Deactivate => "block.beacon.deactivate",
                })
            }
            // The small cube's eating is its own entity's event.
            Sound::SulfurCube(crate::entity::sulfur_cube::Cue::Eat) => s("entity.small_sulfur_cube.eat"),
            Sound::SulfurCube(c) => s(c.key()),
            Sound::Conduit(c) => {
                use crate::world::conduit::Cue as C;
                s(match c {
                    C::Activate => "block.conduit.activate",
                    C::Deactivate => "block.conduit.deactivate",
                    C::Ambient => "block.conduit.ambient",
                    C::AmbientShort => "block.conduit.ambient.short",
                    C::AttackTarget => "block.conduit.attack.target",
                })
            }
            Sound::Geyser(c) => s(c.key()),
            Sound::Mount(c) => s(c.key()),
            Sound::Minecart(c) => {
                use crate::entity::minecart::Cue as C;
                s(match c {
                    C::Rolling => "entity.minecart.riding",
                    C::Inside => "entity.minecart.inside",
                    C::InsideUnderwater => "entity.minecart.inside.underwater",
                })
            }
            Sound::Crafter(c) => {
                use crate::world::crafter::Cue as C;
                s(match c {
                    C::Craft => "block.crafter.craft",
                    C::Fail => "block.crafter.fail",
                })
            }
            // Pairs no `SoundType` plays have no event (and never play).
            Sound::Block(family, cue) => {
                family.has(cue).then(|| format!("block.{}.{}", family.name(), block_cue_event(cue)))
            }
            Sound::Container(c) => s(match c {
                ContainerCue::ChestOpen => "block.chest.open",
                ContainerCue::ChestClose => "block.chest.close",
                ContainerCue::EnderChestOpen => "block.ender_chest.open",
                ContainerCue::EnderChestClose => "block.ender_chest.close",
                ContainerCue::BarrelOpen => "block.barrel.open",
                ContainerCue::BarrelClose => "block.barrel.close",
                ContainerCue::CopperChestOpen => "block.copper_chest.open",
                ContainerCue::CopperChestClose => "block.copper_chest.close",
                ContainerCue::CopperChestWeatheredOpen => "block.copper_chest_weathered.open",
                ContainerCue::CopperChestWeatheredClose => "block.copper_chest_weathered.close",
                ContainerCue::CopperChestOxidizedOpen => "block.copper_chest_oxidized.open",
                ContainerCue::CopperChestOxidizedClose => "block.copper_chest_oxidized.close",
            }),
            Sound::SoulEscape => s("particle.soul_escape"),
        }
    }
}

/// A voice's call: `entity.<mob>.<call>`, or the event Java plays where the
/// mob has no plain one (no ambient call, an idle sound instead, or the
/// creaking's twitch for its hurt).
fn mob_event(v: Voice, c: Call) -> String {
    let special = match (v, c) {
        (Voice::Allay, Call::Ambient) => Some("entity.allay.ambient_without_item"),
        (Voice::Sniffer, Call::Ambient) => Some("entity.sniffer.idle"),
        (Voice::Breeze, Call::Ambient) => Some("entity.breeze.idle_ground"),
        (Voice::Axolotl, Call::Ambient) => Some("entity.axolotl.idle_water"),
        (Voice::Bee, Call::Ambient) => Some("entity.bee.loop"),
        (Voice::Slime, Call::Ambient) => Some("entity.slime.squish"),
        (Voice::SulfurCube, Call::Ambient) => Some("entity.sulfur_cube.squish"),
        // The creeper's ambient voice stands in for its fuse hiss.
        (Voice::Creeper, Call::Ambient) => Some("entity.creeper.primed"),
        (Voice::Creaking, Call::Hurt) => Some("entity.creaking.twitch"),
        _ => None,
    };
    special.map_or_else(|| format!("entity.{}.{}", voice_entity(v), c.name()), str::to_string)
}

fn block_cue_event(cue: BlockCue) -> &'static str {
    match cue {
        BlockCue::Break => "break",
        BlockCue::Place => "place",
        BlockCue::Step => "step",
        BlockCue::Hit => "hit",
        BlockCue::Fall => "fall",
    }
}

/// `SoundType` event family of the legacy shared materials (dirt's is
/// Java's `GRAVEL`, leaves' `GRASS`).
fn material_event(m: Material) -> &'static str {
    match m {
        Material::Stone => "stone",
        Material::Wood => "wood",
        Material::Dirt => "gravel",
        Material::Grass => "grass",
        Material::Gravel => "gravel",
        Material::Sand => "sand",
        Material::Snow => "snow",
        Material::Leaves => "grass",
        Material::Glass => "glass",
        Material::Water => "stone",
        Material::Honey => "honey_block",
    }
}

fn note_event(i: Instrument) -> &'static str {
    use Instrument::*;
    match i {
        Harp => "harp",
        Bass => "bass",
        Snare => "snare",
        Hat => "hat",
        BassDrum => "basedrum",
        Bell => "bell",
        Flute => "flute",
        Chime => "chime",
        Guitar => "guitar",
        Xylophone => "xylophone",
        IronXylophone => "iron_xylophone",
        CowBell => "cow_bell",
        Didgeridoo => "didgeridoo",
        Bit => "bit",
        Banjo => "banjo",
        Pling => "pling",
        Zombie => "imitate.zombie",
        Skeleton => "imitate.skeleton",
        Creeper => "imitate.creeper",
        Dragon => "imitate.ender_dragon",
        WitherSkeleton => "imitate.wither_skeleton",
        Piglin => "imitate.piglin",
    }
}

fn copper_golem_event(a: crate::entity::copper_golem::Sound, stage: u8) -> String {
    use crate::entity::copper_golem::Sound as C;
    // `CopperGolemOxidationLevels`: unaffected and exposed golems share the
    // plain events; weathered and oxidized have their own spin, step, hurt
    // and death.
    let golem = match stage {
        0 | 1 => "copper_golem",
        2 => "copper_golem_weathered",
        _ => "copper_golem_oxidized",
    };
    match a {
        C::Spawn => "entity.copper_golem.spawn".into(),
        C::Spin => format!("entity.{golem}.spin"),
        C::Step => format!("entity.{golem}.step"),
        C::Get => "entity.copper_golem.item_drop".into(),
        C::NoGet => "entity.copper_golem.no_item_get".into(),
        C::Drop => "entity.copper_golem.item_drop".into(),
        C::NoDrop => "entity.copper_golem.item_no_drop".into(),
        C::Shear => "entity.copper_golem.shear".into(),
        C::Statue => "entity.copper_golem_become_statue".into(),
        C::Hurt => format!("entity.{golem}.hurt"),
        C::Death => format!("entity.{golem}.death"),
    }
}

fn sniffer_event(c: crate::entity::sniffer::Cue) -> &'static str {
    use crate::entity::sniffer::Cue as C;
    match c {
        C::Scenting => "entity.sniffer.scenting",
        C::Sniffing => "entity.sniffer.sniffing",
        C::Searching => "entity.sniffer.searching",
        C::Digging => "entity.sniffer.digging",
        C::DiggingStop => "entity.sniffer.digging_stop",
        C::DropSeed => "entity.sniffer.drop_seed",
        C::Happy => "entity.sniffer.happy",
        C::EggCrack => "block.sniffer_egg.crack",
        C::EggHatch => "block.sniffer_egg.hatch",
        C::EggPlop => "block.sniffer_egg.plop",
    }
}

fn warden_event(c: crate::entity::warden::Cue) -> String {
    use crate::entity::warden::Cue as C;
    let name = match c {
        C::Ambient => "ambient",
        C::Agitated => "agitated",
        C::Angry => "angry",
        C::Listening => "listening",
        C::ListeningAngry => "listening_angry",
        C::Heartbeat => "heartbeat",
        C::TendrilClicks => "tendril_clicks",
        C::Roar => "roar",
        C::Sniff => "sniff",
        C::Emerge => "emerge",
        C::Dig => "dig",
        C::AttackImpact => "attack_impact",
        C::SonicCharge => "sonic_charge",
        C::SonicBoom => "sonic_boom",
        C::Hurt => "hurt",
        C::Death => "death",
        C::Step => "step",
        C::NearbyClose => "nearby_close",
        C::NearbyCloser => "nearby_closer",
        C::NearbyClosest => "nearby_closest",
    };
    format!("entity.warden.{name}")
}

fn sculk_event(c: crate::world::sculk::SculkCue) -> &'static str {
    use crate::world::sculk::SculkCue as C;
    match c {
        C::Clicking => "block.sculk_sensor.clicking",
        C::ClickingStop => "block.sculk_sensor.clicking_stop",
        C::Resonate => "block.amethyst_block.resonate",
        C::Bloom => "block.sculk_catalyst.bloom",
        C::Spread => "block.sculk.spread",
        C::Grow => "block.sculk.charge",
        C::Shriek => "block.sculk_shrieker.shriek",
    }
}

fn weapon_event(c: crate::weapons::Cue) -> &'static str {
    use crate::weapons::Cue as C;
    match c {
        C::SmashAir => "item.mace.smash_air",
        C::SmashGround => "item.mace.smash_ground",
        C::SmashHeavy => "item.mace.smash_ground_heavy",
        C::WindBurst => "entity.wind_charge.wind_burst",
        C::TridentThrow => "item.trident.throw",
        C::TridentHit => "item.trident.hit",
        C::TridentHitGround => "item.trident.hit_ground",
        C::TridentReturn => "item.trident.return",
        C::Riptide1 => "item.trident.riptide_1",
        C::Riptide2 => "item.trident.riptide_2",
        C::Riptide3 => "item.trident.riptide_3",
        C::TridentThunder => "item.trident.thunder",
        C::SpearUse => "item.spear.use",
        C::SpearWoodUse => "item.spear_wood.use",
        C::SpearHit => "item.spear.hit",
        C::SpearWoodHit => "item.spear_wood.hit",
        C::SpearAttack => "item.spear.attack",
        C::SpearWoodAttack => "item.spear_wood.attack",
        C::Lunge1 => "item.spear.lunge_1",
        C::Lunge2 => "item.spear.lunge_2",
        C::Lunge3 => "item.spear.lunge_3",
        C::DrownedShoot => "entity.drowned.shoot",
    }
}

fn breeze_event(c: crate::entity::breeze::Cue) -> &'static str {
    use crate::entity::breeze::Cue as C;
    match c {
        C::Whirl => "entity.breeze.whirl",
        C::Shoot => "entity.breeze.shoot",
        C::Inhale => "entity.breeze.inhale",
        C::Charge => "entity.breeze.charge",
        C::Jump => "entity.breeze.jump",
        C::Land => "entity.breeze.land",
        C::Deflect => "entity.breeze.deflect",
        C::Burst => "entity.wind_charge.wind_burst",
        C::Throw => "entity.wind_charge.throw",
        C::SpawnerDetect => "block.trial_spawner.detect_player",
        C::SpawnerSpawn => "block.trial_spawner.spawn_mob",
        C::SpawnerOpen => "block.trial_spawner.open_shutter",
        C::SpawnerEject => "block.trial_spawner.eject_item",
        C::SpawnerClose => "block.trial_spawner.close_shutter",
        C::SpawnerOminous => "block.trial_spawner.ominous_activate",
        C::ItemSpawnerBegin => "block.trial_spawner.spawn_item_begin",
        C::ItemSpawnerDrop => "block.trial_spawner.spawn_item",
        C::VaultActivate => "block.vault.activate",
        C::VaultDeactivate => "block.vault.deactivate",
        C::VaultInsert => "block.vault.insert_item",
        C::VaultFail => "block.vault.insert_item_fail",
        C::VaultReject => "block.vault.reject_rewarded_player",
        C::VaultOpen => "block.vault.open_shutter",
        C::VaultEject => "block.vault.eject_item",
        C::VaultClose => "block.vault.close_shutter",
        C::OminousBottle => "item.ominous_bottle.dispose",
        C::TrialOmen => "event.mob_effect.trial_omen",
    }
}
