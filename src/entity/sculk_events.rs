//! The entity side of sculk and vibrations (v0.16 track A): mobs' steps,
//! swims, landings, damage and deaths become game events
//! (`Entity.move`, `checkFallDamage`, `LivingEntity.hurtServer`, `die`),
//! mobs standing on sensors and shriekers trigger them (`stepOn`), and a
//! catalyst near a death takes its experience
//! ([`crate::world::World::sculk_catalyst_death`]).

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{Entities, MobKind};
use crate::world::World;
use crate::world::block::Block;
use crate::world::vibration::{EntityRef, EventSource, GameEvent};

/// Per-mob movement bookkeeping (Java's `moveDist`, `nextStep` and the fall
/// it is in), not saved.
#[derive(Clone, Copy, Debug, Default)]
struct Watch {
    dying: bool,
    hurt: f32,
    move_dist: f32,
    next_step: f32,
    peak: Option<f64>,
}

#[derive(Default)]
pub struct Sculk {
    /// Loaded catalysts, refreshed from the world every update.
    pub catalysts: Vec<IVec3>,
    /// Deaths a catalyst takes: where, and the experience they would drop.
    pub deaths: Vec<(DVec3, u32)>,
    /// Game events posted where no world is at hand (eating, shooting,
    /// arrows landing), delivered at the next entity update.
    pub events: Vec<(DVec3, GameEvent, EventSource, Option<Block>)>,
    watch: FxHashMap<u32, Watch>,
}

impl Sculk {
    /// Queues a game event for the world (bounded).
    pub fn post(&mut self, pos: DVec3, event: GameEvent, source: EventSource, affected: Option<Block>) {
        if self.events.len() < 256 {
            self.events.push((pos, event, source, affected));
        }
    }

    /// The source of a projectile shot by a player or mob.
    pub fn projectile_source(owner: Option<super::PlayerId>, shooter: Option<u32>) -> EventSource {
        EventSource::projectile(owner.map(EntityRef::Player).or(shooter.map(EntityRef::Mob)))
    }

    /// A loaded catalyst within its 8-block listener radius of `pos`.
    pub fn catalyst_near(&self, pos: DVec3) -> bool {
        let cell = pos.floor().as_ivec3();
        self.catalysts.iter().any(|&c| (c - cell).length_squared() <= 64)
    }
}

/// `Entity.getOnPos`: the block 0.2 below the feet.
pub fn on_pos(feet: DVec3) -> IVec3 {
    (feet - DVec3::Y * 0.2).floor().as_ivec3()
}

impl Entities {
    /// Gives catalysts the deaths waiting since the last update; experience
    /// no catalyst took after all drops as usual.
    pub fn settle_sculk_deaths(&mut self, w: &mut World) {
        for (pos, xp) in std::mem::take(&mut self.sculk.deaths) {
            if !w.sculk_catalyst_death(pos, xp) && xp > 0 {
                self.spawn_xp(pos, xp);
            }
        }
    }

    /// Before the entity update: the world's catalysts and pending deaths.
    pub(super) fn sculk_before(&mut self, w: &mut World) {
        self.sculk.catalysts = w.sculk_catalysts();
        self.settle_sculk_deaths(w);
        self.flush_sculk_events(w);
    }

    /// Delivers the queued game events to the world.
    pub fn flush_sculk_events(&mut self, w: &mut World) {
        for (pos, event, source, affected) in std::mem::take(&mut self.sculk.events) {
            w.emit_game_event(pos, event, source, affected);
        }
    }

    /// After the entity update: what each mob did this tick, as game events.
    pub(super) fn mob_game_events(&mut self, w: &mut World) {
        self.ensure_mob_ids();
        let mut alive = rustc_hash::FxHashSet::default();
        for m in &self.mobs {
            alive.insert(m.uid);
            let source = super::warden::mob_source(m);
            let first = !self.sculk.watch.contains_key(&m.uid);
            let watch = self.sculk.watch.entry(m.uid).or_insert(Watch {
                dying: m.dying.is_some(),
                hurt: m.hurt,
                next_step: 1.0,
                ..Watch::default()
            });
            if !first {
                if m.dying.is_some() && !watch.dying {
                    w.emit_game_event(m.pos, GameEvent::EntityDie, source, None);
                } else if m.alive() && m.hurt > watch.hurt + 1e-4 {
                    w.emit_game_event(m.pos, GameEvent::EntityDamage, source, None);
                }
            }
            watch.dying = m.dying.is_some();
            watch.hurt = m.hurt;
            if !m.alive() || !m.simulation_active || m.riding.is_some() {
                continue;
            }
            let below = on_pos(m.pos);
            let support = w.get_block(below).filter(|&b| b != Block::AIR);
            // `Entity.applyMovementEmissionAndPlaySound`.
            let moved = m.pos - m.previous_pos;
            watch.move_dist += (moved.with_y(0.0).length() * 0.6) as f32;
            if watch.move_dist > watch.next_step
                && let Some(block) = support
            {
                let next =
                    if m.kind == MobKind::Warden { watch.move_dist + 0.55 } else { watch.move_dist.floor() + 1.0 };
                if m.on_ground && !m.in_water {
                    watch.next_step = next;
                    w.emit_game_event(m.pos, GameEvent::Step, source, Some(block));
                } else if m.in_water {
                    watch.next_step = next;
                    w.emit_game_event(m.pos, GameEvent::Swim, source, None);
                }
            }
            // `checkFallDamage`: landing after any fall.
            if m.on_ground {
                if watch.peak.is_some_and(|peak| peak > m.pos.y + 1e-6) && !m.in_water {
                    w.emit_game_event(m.pos, GameEvent::HitGround, source, support);
                }
                watch.peak = None;
            } else if !m.in_water {
                watch.peak = Some(watch.peak.map_or(m.previous_pos.y.max(m.pos.y), |p| p.max(m.pos.y)));
            } else {
                watch.peak = None;
            }
            if m.on_ground {
                w.sculk_step_on(below, m.pos, source, m.kind == MobKind::Warden);
            }
        }
        self.sculk.watch.retain(|uid, _| alive.contains(uid));
    }
}
