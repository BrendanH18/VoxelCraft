//! Grazing, nesting and goat ramming run only for loaded living animals.
use super::*;
use crate::world::{block::Block, overworld_blocks as ob};

impl Entities {
    pub(super) fn tick_animal_lifecycle<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            if !m.alive() || !w.loaded(m.pos.floor().as_ivec3()) {
                continue;
            }
            let cell = m.pos.floor().as_ivec3();
            // Turtles and flying mobs never break turtle eggs. Walking has
            // a 1/100 per-tick roll; landing is handled by the client helper.
            if m.kind != MobKind::Turtle
                && m.kind != MobKind::Parrot
                && m.on_ground
                && let Some(b) = w.block(cell)
                && let Some(count) = ob::egg_count(b)
                && self.rng.chance(dt * 0.2)
            {
                events.push(EntityEvent::AnimalBlock {
                    cell,
                    from: b,
                    to: if count > 1 {
                        ob::turtle_eggs_stage(count - 1, ob::egg_stage(b).unwrap_or(0))
                    } else {
                        Block::AIR
                    },
                });
            }
            let Some(a) = m.animal.as_mut() else { continue };
            if m.kind == MobKind::Sheep {
                if a.grazing > 0.0 {
                    a.grazing = (a.grazing - dt).max(0.0);
                    if a.grazing == 0.0 {
                        let target = if w.block(cell) == Some(Block::TALL_GRASS) {
                            Some((cell, Block::TALL_GRASS, Block::AIR))
                        } else if w.block(cell - IVec3::Y) == Some(Block::GRASS) {
                            Some((cell - IVec3::Y, Block::GRASS, Block::DIRT))
                        } else {
                            None
                        };
                        if let Some((cell, from, to)) = target {
                            if self.villager_griefing {
                                events.push(EntityEvent::AnimalBlock { cell, from, to });
                            }
                            m.sheared = false;
                            if m.age < 0 {
                                m.age = (m.age + 1200).min(0);
                                m.baby = m.age < 0;
                            }
                        }
                    }
                } else if self.rng.chance(dt * if m.baby { 0.4 } else { 0.02 })
                    && (w.block(cell) == Some(Block::TALL_GRASS) || w.block(cell - IVec3::Y) == Some(Block::GRASS))
                {
                    a.grazing = 2.0;
                }
            }
            if m.kind == MobKind::Turtle && a.pregnant {
                if (m.pos - a.home.as_dvec3()).length_squared() > 81.0 {
                    a.goal = Some(a.home.as_dvec3() + DVec3::splat(0.5));
                    a.nest_time = 0.0;
                    continue;
                }
                let mut nest = None;
                for dy in -3..=3 {
                    for x in -4..=4 {
                        for z in -4..=4 {
                            let p = cell + IVec3::new(x, dy, z);
                            if w.loaded(p)
                                && w.block(p) == Some(Block::AIR)
                                && w.block(p - IVec3::Y) == Some(Block::SAND)
                                && (p - a.home).length_squared() <= 81
                                && !events.iter().any(|e| matches!(e,EntityEvent::AnimalBlock{cell,..} if *cell==p))
                            {
                                let d = (p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)).distance_squared(m.pos);
                                if nest.is_none_or(|(_, best)| d < best) {
                                    nest = Some((p, d));
                                }
                            }
                        }
                    }
                }
                if let Some((p, d)) = nest {
                    a.goal = Some(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                    if d < 2.25 && !m.in_water {
                        a.nest_time += dt;
                    } else {
                        a.nest_time = 0.0;
                    }
                    if a.nest_time >= 10.0 {
                        events.push(EntityEvent::AnimalBlock {
                            cell: p,
                            from: Block::AIR,
                            to: ob::turtle_eggs(1 + self.rng.next_int(4) as u8),
                        });
                        a.pregnant = false;
                        a.nest_time = 0.0;
                        a.goal = None;
                    }
                }
            }
            if m.kind == MobKind::Goat {
                a.ram_cooldown = (a.ram_cooldown - dt).max(0.0);
                a.jump_cooldown -= dt;
                if a.jump_cooldown <= 0.0 {
                    a.jump_cooldown = self.rng.range(30.0, 60.0);
                    if m.on_ground && a.love == 0.0 {
                        m.vel.y = 18.0;
                    }
                }
                if a.ramming > 0.0 {
                    a.ramming = (a.ramming - dt).max(0.0);
                    a.ram_prepare = (a.ram_prepare - dt).max(0.0);
                    if a.ram_prepare == 0.0
                        && let Some(goal) = a.ram_goal
                    {
                        let dir = ((goal - m.pos) * DVec3::new(1.0, 0.0, 1.0)).normalize_or(DVec3::X);
                        let ahead = (m.pos + dir * 0.8 + DVec3::Y * 0.5).floor().as_ivec3();
                        if w.block(ahead).is_some_and(Block::is_solid) {
                            if a.horns > 0 && w.block(ahead).is_some_and(horn_breaks_on) {
                                a.horns -= 1;
                                let item = Item(Item::GOAT_HORN.0 + a.horn_kind as u16);
                                self.items.push(super::super::item::ItemEntity::new(
                                    Stack::new(item, 1),
                                    m.pos + DVec3::Y,
                                    dir * -1.5,
                                    0.5,
                                    &mut self.rng,
                                ));
                            }
                            a.ramming = 0.0;
                        } else if let Some(t) =
                            ctx.players.iter().find(|t| t.targetable && t.pos.distance_squared(m.pos) < 2.25)
                        {
                            events.push(EntityEvent::PlayerHit {
                                player: t.id,
                                damage: 2.0,
                                knockback: (dir * 18.0 + DVec3::Y * 4.0).as_vec3(),
                                cause: "was rammed by a goat",
                            });
                            a.ramming = 0.0;
                        }
                    }
                    if a.ramming == 0.0 {
                        a.ram_goal = None;
                        a.ram_cooldown =
                            if a.screaming { self.rng.range(5.0, 15.0) } else { self.rng.range(30.0, 300.0) };
                    }
                } else if a.ram_cooldown == 0.0
                    && !m.baby
                    && a.love == 0.0
                    && let Some(t) = ctx
                        .players
                        .iter()
                        .filter(|t| t.targetable && (16.0..=256.0).contains(&t.pos.distance_squared(m.pos)))
                        .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                {
                    if a.ram_goal.is_some_and(|p| p.distance_squared(t.pos) < 0.01) {
                        a.stationary += dt;
                    } else {
                        a.ram_goal = Some(t.pos);
                        a.stationary = 0.0;
                    }
                    if a.stationary >= 1.0 && super::super::mob::line_of_sight(w, m.pos + DVec3::Y, t.pos + DVec3::Y) {
                        a.ramming = 5.0;
                        a.ram_prepare = 1.0;
                        a.stationary = 0.0;
                    }
                }
            }
        }
    }
}
fn horn_breaks_on(b: Block) -> bool {
    matches!(
        b.base(),
        Block::STONE | Block::IRON_ORE | Block::COPPER_ORE | Block::EMERALD_ORE | crate::world::gadgets::PACKED_ICE
    ) || b.is_log()
}
