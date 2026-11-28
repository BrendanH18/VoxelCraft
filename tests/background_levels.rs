//! v1.0 final F11: a level nobody is in, run as Java 26.3 runs it
//! (`ServerLevel.tick` with no players): its chunk tickets load and tick
//! what they reach, `emptyTime` stops its entities and block entities 300
//! ticks after the last active ticket, and nothing else of it runs. These
//! drive a real headless world through the same steps the game's
//! background levels take (`app::levels::Game::tick_unattended`).
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec2};
use voxelcraft::{
    entity::{Ctx, Entities, EntityEvent, MobKind, PlayerId},
    simulation::{self, TICK_SECONDS, difficulty::Difficulty},
    world::{
        World,
        storage::{LevelInfo, Storage},
        terrain::{Dimension, Generator},
        tickets::{self, EmptyTime, TicketKind, Tickets},
    },
};

/// A level nobody is in: its world, entities, tickets and empty time.
struct Level {
    world: World,
    entities: Entities,
    tickets: Tickets,
    empty: EmptyTime,
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "voxelcraft-background-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ))
}

/// No players: nothing targets anyone and nothing spawns.
fn nobody() -> Ctx {
    Ctx {
        players: Vec::new(),
        daylight: 1.0,
        spawn_light: simulation::lighting::SpawnLight::DAY,
        raining: false,
        spawning: false,
        dimension: Dimension::Overworld,
    }
}

impl Level {
    fn new(seed: u64) -> Self {
        let mut world = World::new_headless(Arc::new(Generator::new(seed)), Default::default(), 2);
        world.set_background(true);
        world.set_simulation_players_with_host(None, std::iter::empty(), std::iter::empty());
        Self { world, entities: Entities::new(seed), tickets: Tickets::default(), empty: EmptyTime::default() }
    }

    /// Streams until what the tickets reach is generated and lit.
    fn settle(&mut self, at: IVec2) {
        self.world.set_tickets(&self.tickets);
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.world.update_background();
            if self.world.pending_jobs() == 0 && self.world.column_loaded(at.x * 16, at.y * 16) {
                self.world.update_block_light();
                if self.world.light_settled() {
                    return;
                }
            }
            assert!(Instant::now() < deadline, "the ticketed chunks did not load");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// One `ServerLevel.tick`, as the game's background levels run it.
    /// Returns whether entities ran, and the entities' events.
    fn tick(&mut self) -> (bool, Vec<EntityEvent>) {
        let world = &self.world;
        self.tickets.purge(|c| world.column_loaded(c.x * 16, c.y * 16));
        self.world.set_tickets(&self.tickets);
        self.world.set_simulation_players_with_host(None, std::iter::empty(), std::iter::empty());
        self.world.update_background();
        let running = self.empty.tick(self.tickets.keep_active());
        simulation::tick_unattended_world_rules(&mut self.world, true, 3, running);
        let events = if running {
            self.world.tick_automation_entities(&mut self.entities);
            self.entities.sync_chunk_entities(&mut self.world);
            self.entities.update_world(TICK_SECONDS, &mut self.world, &nobody(), Difficulty::Normal)
        } else {
            self.entities.sync_chunk_entities(&mut self.world);
            Vec::new()
        };
        for chunk in std::mem::take(&mut self.entities.pearl_tickets) {
            self.tickets.add_with_radius(TicketKind::EnderPearl, chunk, 2);
            self.empty.reset();
        }
        self.entities.clear_pending_sounds();
        self.world.particles.drain().for_each(drop);
        (running, events)
    }

    /// A cow pushed east on the ground at `x`, `z`; its uid.
    fn cow(&mut self, x: i32, z: i32) -> u32 {
        let ground = f64::from(self.world.surface_height(x, z).unwrap() + 1);
        self.entities.spawn(MobKind::Cow, DVec3::new(f64::from(x) + 0.5, ground, f64::from(z) + 0.5));
        self.entities.ensure_mob_ids();
        let m = self.entities.mobs.last_mut().unwrap();
        m.vel = DVec3::new(0.4, 0.0, 0.0);
        m.uid
    }
}

/// Something that came through a portal into a level nobody is in moves
/// there under the arrival's `PORTAL` ticket (radius 3, 300 ticks: its
/// chunk and the eight around it entity-tick), and once the ticket goes
/// its chunks stop ticking and unload, the entity frozen in them.
#[test]
fn an_arrival_moves_for_its_portal_tickets_lifetime_then_freezes_with_its_chunk() {
    let mut level = Level::new(42);
    let portal = IVec2::new(0, 0);
    level.tickets.add_with_radius(TicketKind::Portal, portal, 3);
    level.settle(portal);
    let uid = level.cow(8, 8);
    let start = level.entities.mobs[0].pos;
    let mut ran = 0;
    for _ in 0..600 {
        level.tick();
        if let Some(m) = level.entities.mobs.iter().find(|m| m.uid == uid) {
            ran = m.tick_count;
        }
    }
    // The ticket counted down 300 loaded ticks and went on the 301st.
    assert_eq!(ran, 300, "the cow ticked exactly while the portal ticket lasted");
    assert!(level.tickets.is_empty());
    assert_eq!(level.world.loaded_chunks(), 0, "no ticket: every chunk unloaded");
    assert!(level.entities.mobs.is_empty(), "the cow left the level with its chunk");
    let stored = level.entities.stored_mobs().find(|m| m.uid == uid).expect("frozen in its chunk's storage");
    assert!(stored.pos.distance(start) > 0.1, "it moved while it ticked: {start} to {}", stored.pos);
    assert!(!level.empty.running(), "with nothing active, emptyTime ran out");
}

/// The level the host just left: the players' own tickets went with
/// them, so its entities stop at once, though `emptyTime` has 300 ticks
/// to run; a loading-only ticket (the ground an arrival waits for) loads
/// chunks without ticking them and keeps nothing active, so `emptyTime`
/// runs out 300 ticks later and block entities stop too.
#[test]
fn the_level_left_stops_its_entities_at_once_and_empty_time_runs_out() {
    let mut level = Level::new(7);
    let here = IVec2::new(0, 0);
    level.tickets.add_with_radius(TicketKind::Load, here, 2);
    level.settle(here);
    let uid = level.cow(8, 8);
    let mut running_ticks = 0;
    for _ in 0..400 {
        if level.tick().0 {
            running_ticks += 1;
        }
    }
    let cow = level.entities.mobs.iter().find(|m| m.uid == uid).expect("still loaded");
    assert_eq!(cow.tick_count, 0, "loaded but not entity-ticking: it never moved");
    assert_eq!(running_ticks, 299, "emptyTime ran out 300 ticks after the level emptied");
    // A pearl's ticket keeps the level active again.
    level.tickets.add_with_radius(TicketKind::EnderPearl, tickets::chunk_of(8, 8), 2);
    assert!(level.tick().0);
}

/// A pearl thrown before its owner left flies on in its level under its
/// own `ENDER_PEARL` tickets, renewed as it crosses chunks, and lands for
/// its owner (the game then takes the owner across to it).
#[test]
fn a_pearl_flies_on_under_its_own_tickets_and_lands_for_its_owner() {
    let mut level = Level::new(42);
    let start = tickets::chunk_of(8, 8);
    // The ticket its last tick near its owner left.
    level.tickets.add_with_radius(TicketKind::EnderPearl, start, 2);
    level.settle(start);
    let eye = DVec3::new(8.5, f64::from(level.world.surface_height(8, 8).unwrap()) + 100.0, 8.5);
    level.entities.throw_pearl(PlayerId::HOST, eye, DVec3::X, DVec3::ZERO);
    let mut chunks = std::collections::BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    let landed = loop {
        let (_, events) = level.tick();
        chunks
            .extend(level.tickets.iter().filter(|t| t.kind == TicketKind::EnderPearl).map(|t| (t.chunk.x, t.chunk.y)));
        if let Some(pos) = events.iter().find_map(|e| match e {
            EntityEvent::PearlLanded { owner: PlayerId::HOST, pos } => Some(*pos),
            _ => None,
        }) {
            break pos;
        }
        assert!(Instant::now() < deadline, "the pearl never landed");
        // Its next chunks generate on the workers.
        std::thread::sleep(Duration::from_millis(1));
    };
    let ground = level.world.surface_height(landed.x.floor() as i32, landed.z.floor() as i32);
    assert!(landed.x > 8.0 + 32.0, "it flew on across chunks: landed at {landed} (ground {ground:?})");
    assert!(chunks.len() >= 3, "its tickets followed it: {chunks:?}");
    assert!(level.entities.pearls.is_empty());
}

/// Saving a level running in the background keeps its entities and its
/// portal ticket with the time it has left (`chunk_tickets`, Java's
/// `TicketStorage`); reloaded, it runs on for exactly that time.
#[test]
fn a_background_level_saves_and_runs_on_for_its_tickets_time() {
    let dir = temp_dir("save");
    let portal = IVec2::new(0, 0);
    let uid = {
        let mut level = Level::new(42);
        level.tickets.add_with_radius(TicketKind::Portal, portal, 3);
        level.settle(portal);
        let uid = level.cow(8, 8);
        for _ in 0..100 {
            level.tick();
        }
        assert_eq!(level.tickets.iter().next().unwrap().ticks_left, 200);
        let mut props = BTreeMap::new();
        level.entities.save_props(&mut props);
        props.insert("chunk_tickets".into(), level.tickets.save().unwrap());
        Storage::new(&dir).save_level(&LevelInfo { seed: 42, player: None, props }).unwrap();
        uid
    };
    let saved = Storage::new(&dir).load_level().unwrap();
    let mut level = Level::new(42);
    level.entities.load_props(&saved.props);
    level.tickets = Tickets::load(&saved.props["chunk_tickets"]);
    assert_eq!(level.tickets.iter().next().unwrap().ticks_left, 200);
    level.settle(portal);
    for _ in 0..400 {
        level.tick();
    }
    // `tickCount` isn't saved: it counts the ticks since the reload.
    let cow = level.entities.stored_mobs().find(|m| m.uid == uid).expect("saved, reloaded, then unloaded");
    assert_eq!(cow.tick_count, 200, "it ran on for the ticket's remaining time");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Measures one background level's tick cost: the entities scene's 160
/// mobs inside a portal ticket's area, nobody there. Run with
/// `--ignored --nocapture`; the verification record keeps the numbers.
#[test]
#[ignore]
fn background_level_tick_cost() {
    let mut level = Level::new(20261128);
    let portal = IVec2::new(0, 0);
    level.tickets.add_with_radius(TicketKind::Portal, portal, 3);
    level.settle(portal);
    for i in 0..160 {
        let kind = [MobKind::Cow, MobKind::Sheep, MobKind::Pig, MobKind::Chicken][i % 4];
        let (x, z) = (i as i32 % 16 - 8, i as i32 / 16 - 5);
        let ground = f64::from(level.world.surface_height(x, z).unwrap() + 1);
        level.entities.spawn(kind, DVec3::new(f64::from(x) + 0.5, ground, f64::from(z) + 0.5));
    }
    level.entities.ensure_mob_ids();
    let mut times = Vec::new();
    for t in 0..1200 {
        // Arrivals keep coming: the ticket is renewed.
        if t % 250 == 0 {
            level.tickets.add_with_radius(TicketKind::Portal, portal, 3);
        }
        let begun = Instant::now();
        level.tick();
        if t >= 200 {
            times.push(begun.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let idle: Vec<f64> = {
        level.tickets.remove_kind(TicketKind::Portal);
        (0..400)
            .map(|_| {
                let begun = Instant::now();
                level.tick();
                begun.elapsed().as_secs_f64() * 1000.0
            })
            .skip(350)
            .collect()
    };
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let p99 = times[(times.len() - 1) * 99 / 100];
    let idle_mean = idle.iter().sum::<f64>() / idle.len() as f64;
    println!(
        "background level: {} mobs, {} chunks loaded; tick mean {mean:.3} ms, p99 {p99:.3} ms, max {:.3} ms; with no tickets {idle_mean:.4} ms",
        level.entities.mobs.len() + level.entities.stored_mobs().count(),
        level.world.loaded_chunks(),
        times.last().unwrap()
    );
}
