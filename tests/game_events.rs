//! v0.17 Track E1: Java 26.3 game events for sculk sensors (AUT-01b,
//! sculk part). The registry, frequency table, tags and listener constants
//! are compared with `fixtures/java/26.3/game_events.json` both ways, every
//! Java call site that posts a game event is accounted for, and small
//! scenarios check that the actions the game has post their events with
//! Java's position, source and context. Provenance and differences:
//! `docs/verification/2026-11-23-v0.17-sculk-game-events.md`.
use std::collections::BTreeSet;
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    entity::PlayerId,
    inventory::Stack,
    item::Item,
    world::{
        World,
        block::{Block, Facing},
        cave_blocks::{self as cb, Phase},
        chunk::ChunkData,
        redstone_blocks as r, sculk,
        terrain::Generator,
        vibration::{self, EventSource, GameEvent, Posted},
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/game_events.json")).unwrap()
}

fn all_events() -> Vec<GameEvent> {
    GameEvent::PLAIN.into_iter().chain((1..=15).map(GameEvent::Resonate)).collect()
}

#[test]
fn registry_order_radii_and_frequencies_match_java_both_ways() {
    let f = fixture();
    let java = f["events"].as_array().unwrap();
    assert_eq!(java.len(), 61);
    // Java to the game: every event, in registry order, with its radius and
    // `VIBRATION_FREQUENCY_FOR_EVENT` entry.
    for (i, e) in java.iter().enumerate() {
        let name = e["name"].as_str().unwrap();
        let event = GameEvent::from_name(name).unwrap_or_else(|| panic!("the game lacks {name}"));
        assert_eq!(event.name(), name);
        assert_eq!(i64::from(event.radius()), e["radius"].as_i64().unwrap(), "{name} radius");
        assert_eq!(i64::from(event.frequency()), e["frequency"].as_i64().unwrap(), "{name} frequency");
        if i < GameEvent::PLAIN.len() {
            assert_eq!(GameEvent::PLAIN[i], event, "registry order at {i}");
        } else {
            assert_eq!(event, GameEvent::Resonate((i - GameEvent::PLAIN.len() + 1) as u8));
        }
    }
    // The game to Java: nothing the game has is missing from the registry.
    let names: BTreeSet<&str> = java.iter().map(|e| e["name"].as_str().unwrap()).collect();
    for event in all_events() {
        assert!(names.contains(event.name().as_str()), "{} isn't a Java 26.3 event", event.name());
    }
}

/// A game event tag's name and the game's membership test.
type Tag = (&'static str, fn(GameEvent) -> bool);

#[test]
fn listener_tags_match_java_both_ways() {
    let f = fixture();
    let tags: [Tag; 5] = [
        ("vibrations", GameEvent::is_vibration),
        ("warden_can_listen", GameEvent::warden_listens),
        ("shrieker_can_listen", GameEvent::shrieker_listens),
        ("ignore_vibrations_sneaking", GameEvent::sneaking_ignored),
        ("allay_can_listen", GameEvent::allay_listens),
    ];
    for (tag, listens) in tags {
        let java: BTreeSet<String> =
            f["tags"][tag].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let game: BTreeSet<String> = all_events().into_iter().filter(|e| listens(*e)).map(GameEvent::name).collect();
        assert_eq!(game, java, "#minecraft:{tag}");
        // Each event's own tag list agrees with the resolved tag.
        for e in f["events"].as_array().unwrap() {
            let named = e["tags"].as_array().unwrap().iter().any(|t| t == tag);
            assert_eq!(named, java.contains(e["name"].as_str().unwrap()), "{tag} on {}", e["name"]);
        }
    }
    // Wool occludes; wool and carpets dampen; amethyst blocks resonate.
    assert_eq!(f["block_tags"]["occludes_vibration_signals"], serde_json::json!(["#minecraft:wool"]));
    assert!(vibration::occludes(Block::WOOL) && vibration::dampens(Block::WOOL));
    assert_eq!(f["block_tags"]["vibration_resonators"], serde_json::json!(["minecraft:amethyst_block"]));
}

#[test]
fn listener_radii_timings_and_resonance_pitches_match_java() {
    let f = fixture();
    let l = &f["listeners"];
    let int = |v: &Value| v.as_i64().unwrap();
    assert_eq!(int(&l["sculk_sensor"]["radius"]), i64::from(sculk::SENSOR_RADIUS));
    assert_eq!(int(&l["sculk_sensor"]["active_ticks"]), sculk::SENSOR_ACTIVE_TICKS as i64);
    assert_eq!(int(&l["sculk_sensor"]["cooldown_ticks"]), sculk::SENSOR_COOLDOWN_TICKS as i64);
    assert_eq!(int(&l["calibrated_sculk_sensor"]["radius"]), i64::from(sculk::CALIBRATED_RADIUS));
    assert_eq!(int(&l["calibrated_sculk_sensor"]["active_ticks"]), sculk::CALIBRATED_ACTIVE_TICKS as i64);
    assert_eq!(int(&l["sculk_shrieker"]["radius"]), i64::from(sculk::SHRIEKER_RADIUS));
    assert_eq!(int(&l["sculk_shrieker"]["shrieking_ticks"]), sculk::SHRIEKING_TICKS as i64);
    assert_eq!(int(&l["warden"]["radius"]), i64::from(vibration::WARDEN_LISTENER_RADIUS));
    assert_eq!(int(&l["allay"]["radius"]), i64::from(vibration::ALLAY_LISTENER_RADIUS));
    // `RESONANCE_PITCH_BEND`: `NoteBlock.getPitchFromNote(tone)`.
    for (frequency, tone) in f["resonance_tones"].as_array().unwrap().iter().enumerate() {
        let pitch = 2f32.powf((tone.as_i64().unwrap() - 12) as f32 / 12.0);
        assert!((sculk::resonance_pitch(frequency as u8) - pitch).abs() < 1e-6, "frequency {frequency}");
    }
}

/// How the game stands with each Java method that posts a game event:
/// `posted` (where), `absent` (the game lacks the action) or `gap` (the
/// game has the action but doesn't post the event yet).
const COVERAGE: &[(&str, &str)] = &[
    ("core.cauldron.CauldronInteractions.bootStrap", "posted: interact block_use_event (cauldron bottles)"),
    ("core.cauldron.CauldronInteractions.emptyBucket", "posted: interact block_use_event (cauldron fill)"),
    ("core.cauldron.CauldronInteractions.fillBucket", "posted: interact block_use_event (cauldron take)"),
    ("core.component.BlockTransformer.transformBlock", "posted: interact block_use_event (path, strip, scrape, till)"),
    ("core.dispenser.DispenseItemBehavior$13.execute", "absent: dispensers don't pour water bottles onto dirt"),
    ("core.dispenser.DispenseItemBehavior$4.execute", "posted: automation dispense (bucket pickup)"),
    ("core.dispenser.DispenseItemBehavior$6.execute", "posted: automation dispense (TNT)"),
    ("core.dispenser.DispenseItemBehavior$7.execute", "posted: automation dispense (wither skull)"),
    ("core.dispenser.DispenseItemBehavior$8.execute", "absent: dispensers don't place carved pumpkins"),
    ("core.dispenser.DispenseItemBehavior$9.takeLiquid", "absent: dispensers don't fill glass bottles"),
    ("core.dispenser.FlintAndSteelDispenseItemBehavior.execute", "absent: dispensers don't use flint and steel"),
    ("core.dispenser.ShearsDispenseItemBehavior.tryShearBeehive", "absent: dispensers don't use shears"),
    ("core.dispenser.ShearsDispenseItemBehavior.tryShearEntity", "absent: dispensers don't use shears"),
    ("core.dispenser.SpawnEggItemBehavior.execute", "absent: no spawn eggs"),
    ("server.level.ServerPlayer.die", "posted: sculk player_vitals"),
    ("world.entity.Entity.applyMovementEmissionAndPlaySound", "posted: sculk player_moved, sculk_events (swim)"),
    ("world.entity.Entity.checkFallDamage", "posted: sculk player_moved, sculk_events (hit_ground)"),
    ("world.entity.Entity.doWaterSplashEffect", "posted: sculk player_moved, sculk_events (splash)"),
    ("world.entity.Entity.dropAllLeashConnections", "gap: shearing off leads"),
    ("world.entity.Entity.interact", "gap: leash interactions on non-mob entities"),
    ("world.entity.Entity.kill", "gap: /kill and void removal of non-living entities"),
    ("world.entity.Entity.processFlappingMovement", "gap: per-type flapping (bats, bees, parrots, chickens)"),
    ("world.entity.Entity.removeVehicle", "posted: Entities::dismount (players)"),
    ("world.entity.Entity.restituteMovementAfterCollisions", "posted: sculk player_moved (players); gap for mobs"),
    ("world.entity.Entity.startRiding", "posted: interact entity_use_event (players); gap for mobs"),
    ("world.entity.Entity.vibrationAndSoundEffectsFromBlock", "posted: sculk player_moved, sculk_events (step)"),
    ("world.entity.LightningBolt.tick", "posted: entity lightning"),
    ("world.entity.LivingEntity.actuallyHurt", "posted: sculk_events (mobs), sculk player_vitals (players)"),
    ("world.entity.LivingEntity.checkTotemDeathProtection", "gap: totem use"),
    ("world.entity.LivingEntity.die", "posted: sculk_events (mobs)"),
    ("world.entity.LivingEntity.onEquipItem", "posted: sculk player_vitals (player armor); gap for mobs"),
    ("world.entity.LivingEntity.startUsingItem", "posted: interact item_use_event (fishing); gap for held uses"),
    ("world.entity.LivingEntity.stopUsingItem", "posted: consumption (eating, drinking); gap for bows, shields"),
    ("world.entity.LivingEntity.updateFallFlying", "posted: sculk player_moved (players)"),
    ("world.entity.Mob.ate", "posted: animals lifecycle (sheep grazing)"),
    ("world.entity.Mob.interact", "posted: interact entity_use_event"),
    ("world.entity.Mob.shearItem", "gap: shearing mob equipment"),
    ("world.entity.OminousItemSpawner.spawnItem", "posted: entity trial item spawners"),
    ("world.entity.ai.behavior.HarvestFarmland.tick", "posted: app mobs VillagerHarvest"),
    ("world.entity.ai.behavior.TryLaySpawnOnFluidNearLand.create", "posted: World::animal_block (frogspawn)"),
    ("world.entity.animal.armadillo.Armadillo.brushOffScute", "gap: armadillo brushing"),
    ("world.entity.animal.armadillo.Armadillo.customServerAiStep", "gap: armadillo scute drop"),
    ("world.entity.animal.armadillo.Armadillo.rollOut", "gap: armadillo rolling"),
    ("world.entity.animal.armadillo.Armadillo.rollUp", "gap: armadillo rolling"),
    ("world.entity.animal.camel.Camel.handleEating", "gap: camel feeding"),
    ("world.entity.animal.camel.Camel.handleStartJump", "gap: camel dash"),
    ("world.entity.animal.camel.Camel.sitDown", "gap: camel sitting"),
    ("world.entity.animal.camel.Camel.standUp", "gap: camel sitting"),
    ("world.entity.animal.camel.Camel.standUpInstantly", "gap: camel sitting"),
    ("world.entity.animal.chicken.Chicken.aiStep", "posted: lifecycle LaidEgg"),
    ("world.entity.animal.cow.MushroomCow.mobInteract", "gap: mooshroom shearing (entity_interact is posted)"),
    ("world.entity.animal.equine.AbstractHorse.handleEating", "gap: horse feeding (entity_interact is posted)"),
    ("world.entity.animal.fox.Fox$FoxEatBerriesGoal.pickSweetBerries", "gap: foxes picking berries"),
    ("world.entity.animal.golem.CopperGolem.mobInteract", "gap: copper golem shearing (entity_interact is posted)"),
    ("world.entity.animal.golem.SnowGolem.aiStep", "absent: snow golems leave no snow"),
    ("world.entity.animal.golem.SnowGolem.mobInteract", "gap: snow golem shearing"),
    ("world.entity.animal.nautilus.AbstractNautilus.handleStartJump", "gap: nautilus dash"),
    ("world.entity.animal.panda.Panda.handleEating", "gap: panda eating"),
    ("world.entity.animal.rabbit.Rabbit$RaidGardenGoal.tick", "gap: rabbits eating carrots"),
    ("world.entity.animal.sheep.Sheep.mobInteract", "posted: interact entity_use_event"),
    ("world.entity.animal.sniffer.Sniffer.emitDiggingParticles", "gap: sniffer digging"),
    ("world.entity.animal.turtle.Turtle.aiStep", "gap: turtle egg laying digging"),
    ("world.entity.animal.turtle.Turtle$TurtleLayEggGoal.tick", "posted: World::animal_block (turtle eggs)"),
    ("world.entity.animal.wolf.Wolf.tick", "absent: wolves don't shake off water"),
    ("world.entity.boss.enderdragon.EnderDragon.kill", "gap: dragon removal"),
    ("world.entity.boss.enderdragon.EnderDragon.tickDeath", "gap: dragon death"),
    ("world.entity.decoration.ArmorStand.causeDamage", "gap: armor stand damage"),
    ("world.entity.decoration.ArmorStand.hurtServer", "gap: armor stand hits"),
    ("world.entity.decoration.ArmorStand.kill", "gap: armor stand breaking"),
    ("world.entity.decoration.BlockAttachedEntity.kill", "gap: frames and paintings breaking"),
    ("world.entity.decoration.ItemFrame.dropItem", "gap: frames dropping items when hit"),
    ("world.entity.decoration.ItemFrame.hurtServer", "gap: frames dropping items when hit"),
    ("world.entity.decoration.ItemFrame.interact", "posted: interact entity_use_event"),
    ("world.entity.decoration.LeashFenceKnotEntity.interact", "gap: leads on fences"),
    ("world.entity.item.ItemEntity.hurtServer", "gap: item entities taking damage"),
    ("world.entity.monster.Creeper.tick", "posted: sculk_events (fuse)"),
    ("world.entity.monster.Enderman.teleport", "posted: Mob::teleport"),
    ("world.entity.monster.Enderman$EndermanLeaveBlockGoal.tick", "absent: endermen carry no blocks"),
    ("world.entity.monster.Enderman$EndermanTakeBlockGoal.tick", "absent: endermen carry no blocks"),
    ("world.entity.monster.Ravager.roar", "gap: ravager roar"),
    ("world.entity.monster.Shulker.setRawPeekAmount", "gap: shulker peeking"),
    ("world.entity.monster.Shulker.teleportSomewhere", "posted: shulker_teleport"),
    ("world.entity.monster.Witch.aiStep", "posted: sculk_events (drink)"),
    ("world.entity.monster.creaking.Creaking.activate", "gap: creaking states"),
    ("world.entity.monster.creaking.Creaking.aiStep", "gap: creaking states"),
    ("world.entity.monster.creaking.Creaking.deactivate", "gap: creaking states"),
    ("world.entity.monster.creaking.Creaking.hurtServer", "gap: creaking states"),
    ("world.entity.monster.cubemob.SulfurCube.mobInteract", "gap: sulfur cube shearing (entity_interact is posted)"),
    ("world.entity.monster.cubemob.SulfurCube.primeTime", "gap: sulfur cube fuse"),
    ("world.entity.monster.illager.Evoker$EvokerAttackSpellGoal.createSpellEntity", "gap: evoker fangs"),
    ("world.entity.monster.illager.Evoker$EvokerSummonSpellGoal.performSpellCasting", "gap: evoker vexes"),
    ("world.entity.monster.skeleton.Bogged.mobInteract", "posted: interact entity_use_event"),
    ("world.entity.player.Player.actuallyHurt", "posted: sculk player_vitals"),
    ("world.entity.player.Player.interactOn", "posted: interact entity_use_event"),
    ("world.entity.projectile.FireworkRocketEntity.explode", "posted: firework burst_firework"),
    ("world.entity.projectile.Projectile.onHit", "posted: lifecycle, wither, shulker, trident (land)"),
    ("world.entity.projectile.Projectile.tick", "posted: every projectile spawn (shoot)"),
    ("world.entity.projectile.ShulkerBullet.destroy", "gap: shooting down shulker bullets"),
    ("world.entity.vehicle.VehicleEntity.hurtServer", "gap: hitting boats and minecarts"),
    ("world.entity.vehicle.boat.AbstractBoat.onAboveBubbleColumn", "gap: boats over bubble columns"),
    ("world.entity.vehicle.boat.AbstractChestBoat.interact", "posted: interact entity_use_event"),
    ("world.entity.vehicle.boat.AbstractChestBoat.openCustomInventoryScreen", "posted: interact entity_use_event"),
    ("world.entity.vehicle.boat.AbstractChestBoat.stopOpen", "gap: closing vehicle chests"),
    ("world.entity.vehicle.minecart.MinecartChest.interact", "posted: interact entity_use_event"),
    ("world.entity.vehicle.minecart.MinecartChest.stopOpen", "gap: closing vehicle chests"),
    ("world.item.ArmorStandItem.useOn", "posted: interact block_use_event (decor placed)"),
    ("world.item.BlockItem.place", "posted: placement"),
    ("world.item.BoatItem.use", "posted: interact use_entity (cart placed)"),
    ("world.item.BoneMealItem.useOn", "posted: interact block_use_event"),
    ("world.item.BottleItem.use", "posted: interact item_use_event"),
    ("world.item.BucketItem.playEmptySound", "posted: interact item_use_event, automation dispense"),
    ("world.item.BucketItem.use", "posted: interact item_use_event"),
    ("world.item.CushionItem.useOn", "posted: interact block_use_event"),
    ("world.item.EndCrystalItem.useOn", "posted: interact block_use_event"),
    ("world.item.EnderEyeItem.use", "posted: interact item_use_event"),
    ("world.item.FireChargeItem.useOn", "posted: interact block_use_event (ignited)"),
    ("world.item.FishingRodItem.use", "posted: interact item_use_event"),
    ("world.item.FlintAndSteelItem.useOn", "posted: interact block_use_event (ignited)"),
    ("world.item.HangingEntityItem.useOn", "posted: interact block_use_event (decor placed)"),
    ("world.item.HoneycombItem.useOn", "posted: interact block_use_event (waxed)"),
    ("world.item.InstrumentItem.play", "gap: goat horns"),
    ("world.item.JukeboxPlayable.tryInsertIntoJukebox", "posted: interact block_use_event"),
    ("world.item.JukeboxSongPlayer.stop", "posted: jukebox vibration_hook"),
    ("world.item.JukeboxSongPlayer.tick", "posted: jukebox vibration_hook"),
    ("world.item.LeadItem.bindPlayerMobs", "gap: leads on fences"),
    ("world.item.MinecartItem.useOn", "posted: interact use_entity (cart placed)"),
    ("world.item.MobBucketItem.checkExtraContent", "posted: automation dispense; gap for players"),
    ("world.item.PotionItem.useOn", "gap: water bottles onto dirt"),
    ("world.item.ShearsItem.useOn", "gap: trimming plant tips"),
    ("world.item.SolidBucketItem.emptyContents", "posted: automation dispense; interact item_use_event"),
    ("world.item.SpawnEggItem.spawnMob", "absent: no spawn eggs"),
    ("world.item.SpawnEggItem.useOn", "absent: no spawn eggs"),
    ("world.item.component.Consumable.onConsume", "posted: consumption"),
    ("world.item.consume_effects.TeleportRandomlyConsumeEffect.apply", "posted: consumption chorus_teleport"),
    ("world.item.enchantment.Enchantments.bootstrap", "gap: frost walker"),
    ("world.item.enchantment.effects.SetBlockProperties.<init>", "absent: no enchantment sets block properties"),
    ("world.level.BaseSpawner.serverTick", "gap: monster spawners"),
    ("world.level.Level.destroyBlock", "gap: blocks mobs, falling and fluids break (mining posts its own)"),
    ("world.level.ServerExplosion.explode", "posted: app mobs and wither explosions"),
    ("world.level.block.AbstractCandleBlock.extinguish", "posted: interact block_use_event"),
    ("world.level.block.BasePressurePlateBlock.checkPressed", "posted: redstone_contacts, redstone tick"),
    ("world.level.block.BeehiveBlock.useItemOn", "gap: harvesting beehives"),
    ("world.level.block.BellBlock.attemptToRing", "posted: World::ring_bell_by"),
    ("world.level.block.BigDripleafBlock.setTilt", "gap: dripleaf tilting"),
    ("world.level.block.Block.playerWillDestroy", "posted: mining"),
    ("world.level.block.BrushableBlock.onBrokenAfterFall", "gap: falling suspicious blocks"),
    ("world.level.block.ButtonBlock.checkPressed", "posted: redstone_contacts (arrows), redstone tick (release)"),
    ("world.level.block.ButtonBlock.press", "posted: World::use_redstone_by"),
    ("world.level.block.CakeBlock.eat", "posted: interact block_use_event"),
    ("world.level.block.CakeBlock.useItemOn", "posted: interact block_use_event"),
    ("world.level.block.CampfireBlock.douse", "posted: interact block_use_event (shovels); gap for splashes"),
    ("world.level.block.CauldronBlock.handlePrecipitation", "gap: cauldrons filling in rain"),
    ("world.level.block.CauldronBlock.receiveStalactiteDrip", "gap: dripstone filling cauldrons"),
    ("world.level.block.CaveVines.use", "posted: interact block_use_event (harvested)"),
    ("world.level.block.ChiseledBookShelfBlock.removeBook", "posted: interact block_use_event"),
    ("world.level.block.ComposterBlock.addLayer", "posted: interact block_use_event"),
    ("world.level.block.ComposterBlock.empty", "posted: interact block_use_event"),
    ("world.level.block.CopperGolemStatueBlock.updatePose", "posted: interact block_use_event"),
    ("world.level.block.DaylightDetectorBlock.useWithoutItem", "posted: World::use_redstone_by"),
    ("world.level.block.DecoratedPotBlock.useItemOn", "posted: interact block_use_event"),
    ("world.level.block.DecoratedPotBlock.useWithoutItem", "gap: wobbling decorated pots"),
    ("world.level.block.DispenserBlock.dispenseFrom", "posted: automation dispense"),
    ("world.level.block.DoorBlock.neighborChanged", "posted: redstone update"),
    ("world.level.block.DoorBlock.setOpen", "posted: wind_trigger"),
    ("world.level.block.DoorBlock.useWithoutItem", "posted: interact block_use_event"),
    ("world.level.block.DriedGhastBlock.tick", "gap: dried ghast hydration"),
    ("world.level.block.DriedGhastBlock.tickWaterlogged", "gap: dried ghast hydration"),
    ("world.level.block.EyeblossomBlock.tryChangingState", "gap: eyeblossoms opening and closing"),
    ("world.level.block.FarmlandBlock.turnToBaseBlock", "gap: farmland trampling and drying"),
    ("world.level.block.FenceGateBlock.neighborChanged", "posted: redstone update"),
    ("world.level.block.FenceGateBlock.onExplosionHit", "posted: wind_trigger"),
    ("world.level.block.FenceGateBlock.useWithoutItem", "posted: interact block_use_event"),
    ("world.level.block.FlowerPotBlock.useItemOn", "posted: interact block_use_event"),
    ("world.level.block.FlowerPotBlock.useWithoutItem", "posted: interact block_use_event"),
    ("world.level.block.IceBlock.melt", "absent: ice doesn't melt by light"),
    ("world.level.block.LayeredCauldronBlock.handlePrecipitation", "gap: cauldrons filling in snow and rain"),
    ("world.level.block.LayeredCauldronBlock.lowerFillLevel", "posted: interact block_use_event (bottles, washing)"),
    ("world.level.block.LayeredCauldronBlock.receiveStalactiteDrip", "gap: dripstone filling cauldrons"),
    ("world.level.block.LecternBlock.resetBookState", "posted: interact block_use_event (book placed)"),
    ("world.level.block.LeverBlock.pull", "posted: World::use_redstone_by"),
    ("world.level.block.NoteBlock.playNote", "posted: gadgets play_note"),
    ("world.level.block.PathBlock.turnToBaseBlock", "gap: paths reverting under blocks"),
    ("world.level.block.PointedDripstoneBlock.maybeTransferFluid", "gap: dripstone transferring fluid"),
    ("world.level.block.PotentSulfurBlock.onPlace", "posted: potent_sulfur"),
    ("world.level.block.PumpkinBlock.useItemOn", "posted: interact block_use_event (carved)"),
    ("world.level.block.RespawnAnchorBlock.charge", "posted: interact block_use_event"),
    ("world.level.block.SculkSensorBlock.activate", "posted: sculk activate_sensor"),
    ("world.level.block.SculkSensorBlock.stepOn", "posted: sculk sculk_step_on"),
    ("world.level.block.ShelfBlock.neighborChanged", "gap: powered shelves"),
    ("world.level.block.ShelfBlock.swapHotbar", "posted: interact block_use_event"),
    ("world.level.block.ShelfBlock.swapSingleItem", "posted: interact block_use_event"),
    ("world.level.block.SignBlock.useItemOn", "posted: interact block_use_event"),
    ("world.level.block.SnifferEggBlock.onPlace", "gap: sniffer eggs"),
    ("world.level.block.SnowLayerBlock.randomTick", "absent: snow layers don't melt by light"),
    ("world.level.block.SweetBerryBushBlock.randomTick", "gap: berry bushes growing"),
    ("world.level.block.SweetBerryBushBlock.useWithoutItem", "posted: interact block_use_event (harvested)"),
    ("world.level.block.TntBlock.prime", "posted: interact, redstone update, fire"),
    ("world.level.block.TrapDoorBlock.playSound", "posted: World::use_redstone_by, redstone update"),
    ("world.level.block.TripWireBlock.playerWillDestroy", "posted: mining"),
    ("world.level.block.TripWireHookBlock.emitState", "posted: gadgets connect_hook"),
    ("world.level.block.TurtleEggBlock.decreaseEggs", "posted: World::animal_block"),
    ("world.level.block.TurtleEggBlock.randomTick", "gap: turtle eggs cracking and hatching"),
    ("world.level.block.entity.BeehiveBlockEntity.addOccupant", "gap: bees entering hives"),
    ("world.level.block.entity.BeehiveBlockEntity.releaseOccupant", "gap: bees leaving hives"),
    ("world.level.block.entity.CampfireBlockEntity.cookTick", "gap: campfires finishing food"),
    ("world.level.block.entity.CampfireBlockEntity.placeFood", "posted: interact block_use_event"),
    ("world.level.block.entity.ChiseledBookShelfBlockEntity.updateState", "posted: interact block_use_event"),
    ("world.level.block.entity.ContainerOpenersCounter.decrementOpeners", "posted: World::set_chest_viewers (chests)"),
    ("world.level.block.entity.ContainerOpenersCounter.incrementOpeners", "posted: interact block_use_event"),
    ("world.level.block.entity.ContainerOpenersCounter.recheckOpeners", "posted: World::set_golem_chest_viewers"),
    ("world.level.block.entity.CreakingHeartBlockEntity.creakingHurt", "gap: creaking heart resin"),
    ("world.level.block.entity.CreakingHeartBlockEntity.spawnProtector", "gap: creaking spawns"),
    ("world.level.block.entity.JukeboxBlockEntity.notifyItemChangedInJukebox", "posted: interact block_use_event"),
    ("world.level.block.entity.JukeboxBlockEntity.setRemoved", "posted: jukebox vibration_hook"),
    ("world.level.block.entity.PotentSulfurBlockEntity.<clinit>", "posted: potent_sulfur"),
    ("world.level.block.entity.SculkShriekerBlockEntity.shriek", "posted: sculk shriek"),
    ("world.level.block.entity.ShelfBlockEntity.setChanged", "posted: interact block_use_event"),
    ("world.level.block.entity.ShulkerBoxBlockEntity.startOpen", "posted: interact block_use_event"),
    ("world.level.block.entity.ShulkerBoxBlockEntity.stopOpen", "posted: shulker_box"),
    ("world.level.block.entity.trialspawner.TrialSpawner.spawnMob", "gap: trial spawners"),
    ("world.level.block.piston.PistonBaseBlock.moveBlocks", "posted: automation extend_piston"),
    ("world.level.block.piston.PistonBaseBlock.triggerEvent", "posted: automation extend_piston, retract_piston"),
];

#[test]
fn every_java_call_site_is_accounted_for() {
    let f = fixture();
    let java: BTreeSet<String> = f["call_sites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| format!("{}.{}", c["class"].as_str().unwrap(), c["method"].as_str().unwrap()))
        .collect();
    let table: BTreeSet<String> = COVERAGE.iter().map(|(site, _)| site.to_string()).collect();
    assert_eq!(table.len(), COVERAGE.len(), "a call site is listed twice");
    let missing: Vec<_> = java.difference(&table).collect();
    let stale: Vec<_> = table.difference(&java).collect();
    assert!(missing.is_empty() && stale.is_empty(), "unlisted Java call sites {missing:?}; stale rows {stale:?}");
    for (site, status) in COVERAGE {
        assert!(
            ["posted: ", "absent: ", "gap: "].iter().any(|p| status.starts_with(p)),
            "{site} has an unknown status {status:?}"
        );
    }
    // Every event a call site names exists in the game.
    for c in f["call_sites"].as_array().unwrap() {
        for e in c["events"].as_array().unwrap() {
            assert!(GameEvent::from_name(e.as_str().unwrap()).is_some(), "{e}");
        }
    }
}

// Scenarios. A headless world with one air chunk and a stone floor; entity
// listeners on, so every posted event can be read back.

const P: IVec3 = IVec3::new(8, 145, 8);
const PLAYER: PlayerId = PlayerId(7);

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut w = World::new_headless(Arc::new(Generator::new(7)), saved, 2);
    for _ in 0..100_000 {
        w.update(DVec3::new(16.0, 146.0, 16.0));
        if w.is_loaded(P) {
            break;
        }
        std::thread::yield_now();
    }
    assert!(w.is_loaded(P), "fixture chunk didn't load");
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
        }
    }
    w.set_entity_listeners(true);
    w.take_entity_game_events();
    w
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
        w.tick_sculk();
    }
}

/// The events posted at `cell` (by the cell holding their position).
fn at(w: &mut World, cell: IVec3) -> Vec<Posted> {
    w.take_entity_game_events().into_iter().filter(|p| p.pos.floor().as_ivec3() == cell).collect()
}

fn kinds(posted: &[Posted]) -> Vec<GameEvent> {
    posted.iter().map(|p| p.event).collect()
}

fn player() -> EventSource {
    EventSource::player(PLAYER, false)
}

#[test]
fn redstone_uses_post_from_the_player_and_repeaters_post_nothing() {
    let mut w = world();
    // Mount 0: on the floor.
    let lever = P;
    w.set_block(lever, r::lever(0, false));
    assert!(w.use_redstone_by(lever, player()));
    let on = at(&mut w, lever);
    assert_eq!(kinds(&on), [GameEvent::BlockActivate]);
    assert_eq!(on[0].source.player, Some(PLAYER));
    assert_eq!(on[0].pos, lever.as_dvec3() + DVec3::splat(0.5), "posted at the block centre");
    assert!(w.use_redstone_by(lever, player()));
    assert_eq!(kinds(&at(&mut w, lever)), [GameEvent::BlockDeactivate]);
    // `RepeaterBlock` and `ComparatorBlock.useWithoutItem` post nothing.
    let repeater = P + IVec3::new(3, 0, 0);
    w.set_block(repeater, r::repeater(Facing::South, 1, false));
    assert!(w.use_redstone_by(repeater, player()));
    assert!(kinds(&at(&mut w, repeater)).is_empty());
    // `DaylightDetectorBlock.useWithoutItem`: `block_change` naming the new state.
    let detector = P + IVec3::new(5, 0, 0);
    w.set_block(detector, r::daylight(0, false));
    assert!(w.use_redstone_by(detector, player()));
    let changed = at(&mut w, detector);
    assert_eq!(kinds(&changed), [GameEvent::BlockChange]);
    assert_eq!(changed[0].affected, Some(r::daylight(0, true)));
    // `TrapDoorBlock.playSound` from a use.
    let trapdoor = P + IVec3::new(7, 0, 0);
    w.set_block(trapdoor, r::trapdoor(Facing::South, false, false, false));
    assert!(w.use_redstone_by(trapdoor, player()));
    assert_eq!(kinds(&at(&mut w, trapdoor)), [GameEvent::BlockOpen]);
    // `BellBlock.attemptToRing`.
    let bell = P + IVec3::new(9, 0, 0);
    w.set_block(bell, Block::BELL);
    assert!(w.ring_bell_by(bell, player()));
    assert_eq!(kinds(&at(&mut w, bell)), [GameEvent::BlockChange]);
    // `NoteBlock.playNote`: the player who struck it is the source.
    let note = P + IVec3::new(11, 0, 0);
    w.set_block(note, voxelcraft::world::gadgets::note(0, false));
    assert!(w.strike_note_by(note, player()));
    let played = at(&mut w, note);
    assert_eq!(kinds(&played), [GameEvent::NoteBlockPlay]);
    assert_eq!(played[0].source.player, Some(PLAYER));
}

#[test]
fn buttons_release_and_plates_press_and_release_with_their_sources() {
    let mut w = world();
    let button = P;
    w.set_block(button, r::button(0, false, false));
    assert!(w.use_redstone_by(button, player()));
    assert_eq!(kinds(&at(&mut w, button)), [GameEvent::BlockActivate]);
    // A stone button releases after 20 ticks: `checkPressed` with no source.
    ticks(&mut w, 25);
    let released = at(&mut w, button);
    assert_eq!(kinds(&released), [GameEvent::BlockDeactivate]);
    assert_eq!(released[0].source, EventSource::NONE);
    // A player on a stone plate presses it, naming them.
    let plate = P + IVec3::new(4, 0, 0);
    w.set_block(plate, r::plate(0, 0));
    let entities = voxelcraft::entity::Entities::new(1);
    let feet = plate.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    w.redstone_contacts_from([(feet, voxelcraft::player::SHAPE, player())], &entities);
    let pressed = at(&mut w, plate);
    assert_eq!(kinds(&pressed), [GameEvent::BlockActivate]);
    assert_eq!(pressed[0].source.player, Some(PLAYER));
    // Stepping off: the plate's own tick releases it with no source.
    for _ in 0..30 {
        w.redstone_contacts([], &entities);
        ticks(&mut w, 1);
    }
    assert_eq!(kinds(&at(&mut w, plate)), [GameEvent::BlockDeactivate]);
}

#[test]
fn powered_doors_trapdoors_and_tnt_post_without_a_source() {
    let mut w = world();
    let trapdoor = P + IVec3::Y;
    w.set_block(trapdoor, r::trapdoor(Facing::South, false, false, true));
    w.set_block(trapdoor + IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 2);
    let opened = at(&mut w, trapdoor);
    assert_eq!(kinds(&opened), [GameEvent::BlockOpen]);
    assert_eq!(opened[0].source, EventSource::NONE);
    w.set_block(trapdoor + IVec3::X, Block::AIR);
    ticks(&mut w, 2);
    assert_eq!(kinds(&at(&mut w, trapdoor)), [GameEvent::BlockClose]);
    // `TntBlock.prime` from a neighbour signal.
    let tnt = P + IVec3::new(6, 0, 0);
    w.set_block(tnt, Block::TNT);
    w.set_block(tnt + IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 2);
    assert_eq!(kinds(&at(&mut w, tnt)), [GameEvent::PrimeFuse]);
}

#[test]
fn pistons_post_activate_deactivate_and_the_blocks_they_destroy() {
    let mut w = world();
    let piston = P;
    w.set_block(piston, r::piston(2, false, false));
    let lantern = voxelcraft::world::mount_states::LANTERN;
    w.set_block(piston + IVec3::X, lantern);
    w.take_entity_game_events();
    w.set_block(piston - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 3);
    let destroyed = at(&mut w, piston + IVec3::X);
    assert!(
        destroyed.iter().any(|p| p.event == GameEvent::BlockDestroy && p.affected == Some(lantern)),
        "{destroyed:?}"
    );
    w.take_entity_game_events();
    w.set_block(piston - IVec3::X, Block::AIR);
    ticks(&mut w, 3);
    let retracted = at(&mut w, piston);
    assert_eq!(kinds(&retracted), [GameEvent::BlockDeactivate]);
    w.set_block(piston - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 3);
    let extended = at(&mut w, piston);
    assert_eq!(kinds(&extended), [GameEvent::BlockActivate]);
    assert_eq!(extended[0].affected, Some(r::piston(2, false, true)));
}

#[test]
fn dispensers_click_empty_and_post_their_bucket_behaviours() {
    let mut w = world();
    let d = P;
    w.set_block(d, r::dispenser(2, false));
    w.set_block(d - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert_eq!(kinds(&at(&mut w, d)), [GameEvent::BlockActivate], "an empty dispenser clicks");
    w.set_block(d - IVec3::X, Block::AIR);
    ticks(&mut w, 2);
    w.chest_mut(d).unwrap().slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    w.set_block(d - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert_eq!(kinds(&at(&mut w, d + IVec3::X)), [GameEvent::FluidPlace]);
    w.set_block(d - IVec3::X, Block::AIR);
    ticks(&mut w, 2);
    w.set_block(d - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert_eq!(kinds(&at(&mut w, d + IVec3::X)), [GameEvent::FluidPickup]);
    // A dropper with nothing in it posts nothing.
    let dropper = P + IVec3::new(0, 0, 6);
    w.set_block(dropper, r::dispenser(2, true));
    w.set_block(dropper - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 5);
    assert!(kinds(&at(&mut w, dropper)).is_empty());
}

#[test]
fn tripwire_hooks_post_attach_then_activate_at_both_ends() {
    use voxelcraft::world::gadgets::{hook, tripwire};
    let mut w = world();
    let (a, b) = (P + IVec3::Y, P + IVec3::new(4, 1, 0));
    w.set_block(a - IVec3::X, Block::STONE);
    w.set_block(b + IVec3::X, Block::STONE);
    w.set_block(a, hook(Facing::East, false, false));
    w.set_block(b, hook(Facing::West, false, false));
    for x in 1..4 {
        w.set_block(a + IVec3::X * x, tripwire(false, false, false));
    }
    ticks(&mut w, 2);
    let events = w.take_entity_game_events();
    for end in [a, b] {
        assert!(
            events.iter().any(|p| p.event == GameEvent::BlockAttach && p.pos.floor().as_ivec3() == end),
            "{end}: {events:?}"
        );
    }
    let entities = voxelcraft::entity::Entities::new(1);
    let on_wire = (a + IVec3::X * 2).as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    w.redstone_contacts_from([(on_wire, voxelcraft::player::SHAPE, player())], &entities);
    ticks(&mut w, 2);
    let events = w.take_entity_game_events();
    for end in [a, b] {
        assert!(
            events.iter().any(|p| p.event == GameEvent::BlockActivate && p.pos.floor().as_ivec3() == end),
            "{end}: {events:?}"
        );
    }
}

#[test]
fn a_sensor_hears_a_piston_with_java_power_frequency_and_timing() {
    let mut w = world();
    let sensor = P + IVec3::new(5, 0, 0);
    w.set_block(sensor, cb::SCULK_SENSOR);
    let piston = P;
    w.set_block(piston, r::piston(4, false, false));
    w.set_block(piston - IVec3::X, r::REDSTONE_BLOCK);
    // The piston extends on its block event; the sensor picks the
    // candidate a tick later and the vibration travels floor(5) ticks.
    let mut active_at = None;
    for t in 0..20 {
        ticks(&mut w, 1);
        if cb::sensor_of(w.get_block(sensor).unwrap()).is_some_and(|(phase, _)| phase == Phase::Active) {
            active_at = Some(t);
            break;
        }
    }
    let active_at = active_at.expect("the sensor never heard the piston");
    assert!((5..=9).contains(&active_at), "activated after {active_at} ticks");
    let e = w.sculk_entity(sensor).unwrap();
    assert_eq!(e.last_frequency, GameEvent::BlockActivate.frequency());
    assert_eq!(e.power, vibration::redstone_strength(5.0, sculk::SENSOR_RADIUS));
    // Active for 30 ticks, cooling down for 10, then listening again.
    ticks(&mut w, sculk::SENSOR_ACTIVE_TICKS as usize - 1);
    assert_eq!(cb::sensor_of(w.get_block(sensor).unwrap()).unwrap().0, Phase::Active);
    ticks(&mut w, 1);
    assert_eq!(cb::sensor_of(w.get_block(sensor).unwrap()).unwrap().0, Phase::Cooldown);
    ticks(&mut w, sculk::SENSOR_COOLDOWN_TICKS as usize);
    assert_eq!(cb::sensor_of(w.get_block(sensor).unwrap()).unwrap().0, Phase::Inactive);
}

#[test]
fn players_splash_bounce_glide_and_take_damage_equip_and_die() {
    let mut w = world();
    let mut p = voxelcraft::player::Player::new(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    p.on_ground = true;
    let before = p.pos;
    w.player_moved(PLAYER, &p, before, false, false);
    p.in_water = true;
    p.on_ground = false;
    w.player_moved(PLAYER, &p, before, false, false);
    assert!(kinds(&w.take_entity_game_events()).contains(&GameEvent::Splash));
    p.in_water = false;
    p.on_ground = true;
    p.bounced = true;
    w.player_moved(PLAYER, &p, before, false, false);
    assert!(kinds(&w.take_entity_game_events()).contains(&GameEvent::Bounce));
    p.bounced = false;
    p.on_ground = false;
    p.fall_flying = true;
    let glides = (0..20)
        .filter(|_| {
            w.player_moved(PLAYER, &p, p.pos, false, false);
            kinds(&w.take_entity_game_events()).contains(&GameEvent::ElytraGlide)
        })
        .count();
    assert_eq!(glides, 2, "every tenth tick of gliding");
    let source = player();
    let at_feet = p.pos;
    w.player_vitals(PLAYER, at_feet, 20.0, [None; 4], source);
    assert!(w.take_entity_game_events().is_empty(), "nothing the first time");
    let helmet = Item::from_name("iron_helmet").unwrap();
    w.player_vitals(PLAYER, at_feet, 17.0, [None, None, None, Some(helmet)], source);
    assert_eq!(kinds(&w.take_entity_game_events()), [GameEvent::EntityDamage, GameEvent::Equip]);
    w.player_vitals(PLAYER, at_feet, 17.0, [None; 4], source);
    assert_eq!(kinds(&w.take_entity_game_events()), [GameEvent::Unequip]);
    w.player_vitals(PLAYER, at_feet, 0.0, [None; 4], source);
    assert_eq!(kinds(&w.take_entity_game_events()), [GameEvent::EntityDie]);
}

#[test]
fn mob_block_changes_post_place_and_destroy() {
    let mut w = world();
    let egg = P + IVec3::Y;
    let eggs = voxelcraft::world::overworld_blocks::turtle_eggs(2);
    assert!(w.animal_block(egg, Block::AIR, eggs));
    let laid = at(&mut w, egg);
    assert_eq!(kinds(&laid), [GameEvent::BlockPlace]);
    assert_eq!(laid[0].affected, Some(eggs));
    let fewer = voxelcraft::world::overworld_blocks::turtle_eggs(1);
    assert!(w.animal_block(egg, eggs, fewer));
    assert_eq!(kinds(&at(&mut w, egg)), [GameEvent::BlockDestroy], "`decreaseEggs`");
    // A sheep's grass turning to dirt is only the sheep's `eat`.
    let grass = P + IVec3::new(3, 0, 0);
    w.set_block(grass, Block::GRASS);
    w.take_entity_game_events();
    assert!(w.animal_block(grass, Block::GRASS, Block::DIRT));
    assert!(at(&mut w, grass).is_empty());
}

#[test]
fn player_uses_post_block_transforms_and_lever_pulls_from_the_player() {
    use voxelcraft::item::{Tier, ToolKind};
    use voxelcraft::session::{PlayerSession, interact::UseContext};
    let mut w = world();
    let mut e = voxelcraft::entity::Entities::new(1);
    let under = P - IVec3::Y;
    w.set_block(under, Block::GRASS);
    let mut s = PlayerSession::new(PLAYER, P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.player.on_ground = true;
    s.player.pitch = -1.5;
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Hoe, Tier::Iron), 1));
    w.take_entity_game_events();
    let ctx = UseContext { sneaking: false, others: &[] };
    assert!(s.use_held(0, &mut w, &mut e, &ctx).unwrap().is_some());
    // `BlockTransformer.transformBlock`: `block_change` naming the new state.
    let tilled = at(&mut w, under);
    assert_eq!(kinds(&tilled), [GameEvent::BlockChange]);
    assert_eq!(tilled[0].affected, Some(Block::FARMLAND));
    assert_eq!(tilled[0].source.player, Some(PLAYER));
    // A lever pulled through the session names the player once.
    w.set_block(under, Block::STONE);
    w.set_block(P + IVec3::X, r::lever(0, false));
    s.inventory.slots[0] = None;
    s.player.pitch = -0.9;
    s.player.yaw = 0.0;
    let mut pulled = Vec::new();
    for k in 0..40 {
        s.player.pitch = -0.4 - k as f32 * 0.03;
        if s.use_held(0, &mut w, &mut e, &ctx).ok().flatten().is_some() {
            pulled = at(&mut w, P + IVec3::X);
            break;
        }
    }
    assert_eq!(kinds(&pulled), [GameEvent::BlockActivate]);
    assert_eq!(pulled[0].source.player, Some(PLAYER));
}
