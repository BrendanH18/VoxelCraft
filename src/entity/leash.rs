//! Java 1.21 player leads and fence knots. Anchors use stable profile ids.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{
    inventory::{Stack, StackName},
    item::Item,
    world::block::{Block, Shaped},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leash {
    Player(PlayerId),
    Fence(IVec3),
}
impl MobKind {
    pub fn leashable(self) -> bool {
        matches!(
            self,
            Self::Cow
                | Self::Sheep
                | Self::Pig
                | Self::Chicken
                | Self::Cat
                | Self::Wolf
                | Self::Fox
                | Self::Parrot
                | Self::Rabbit
                | Self::Goat
                | Self::Hoglin
                | Self::Zoglin
                | Self::Strider
                | Self::TraderLlama
                | Self::IronGolem
                | Self::SnowGolem
                | Self::Dolphin
                | Self::Axolotl
        )
    }
}
pub fn fence(b: Block) -> bool {
    matches!(b.shaped(), Some(Shaped::Fence))
}
impl Entities {
    pub(super) fn use_lead_or_name(&mut self, index: usize, held: Option<Stack>, player: PlayerId) -> Option<bool> {
        let held = held?;
        let m = self.mobs.get_mut(index)?;
        if !m.alive() {
            return None;
        }
        if held.item == Item::NAME_TAG {
            held.name.as_str()?;
            m.name = held.name;
            m.persistent = true;
            return Some(true);
        }
        // Using a leashed animal takes its lead off, even with an empty
        // hand; keep owner sit commands available when it has no lead.
        if held.item == Item::LEAD && m.kind.leashable() {
            if m.leash.is_some() {
                let pos = m.pos;
                m.leash = None;
                m.leash_pos = None;
                self.scatter(Stack::new(Item::LEAD, 1), pos);
                return Some(false);
            }
            m.leash = Some(Leash::Player(player));
            m.persistent = true;
            if let Some(a) = &mut m.animal {
                a.perched = None;
            }
            return Some(true);
        }
        None
    }
    /// Clicking a fence transfers all of this player's nearby leads to a
    /// single knot; clicking an existing knot releases its leads.
    pub fn use_fence_knot(&mut self, player: PlayerId, p: IVec3) -> bool {
        let mut tied = false;
        for m in &mut self.mobs {
            if m.alive()
                && m.leash == Some(Leash::Player(player))
                && m.pos.distance_squared(p.as_dvec3() + DVec3::splat(0.5)) <= 49.0
            {
                m.leash = Some(Leash::Fence(p));
                tied = true;
            }
        }
        tied || self.break_fence_knot(p)
    }
    pub fn break_fence_knot(&mut self, p: IVec3) -> bool {
        let mut count = 0;
        for m in &mut self.mobs {
            if m.leash == Some(Leash::Fence(p)) {
                m.leash = None;
                m.leash_pos = None;
                count += 1;
            }
        }
        for _ in 0..count {
            self.scatter(Stack::new(Item::LEAD, 1), p.as_dvec3() + DVec3::new(0.5, 0.7, 0.5));
        }
        count > 0
    }
    pub(super) fn tick_leashes<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W, ctx: &Ctx) {
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            let Some(leash) = m.leash else { continue };
            if !w.loaded(m.pos.floor().as_ivec3()) {
                continue;
            }
            let (anchor, broken) = match leash {
                Leash::Player(id) => {
                    // Offline profiles retain attachments across saves.
                    let Some(t) = ctx.players.iter().find(|t| t.id == id) else {
                        m.leash_pos = None;
                        continue;
                    };
                    (t.pos + DVec3::Y * 1.2, !t.alive)
                }
                Leash::Fence(p) => {
                    if !w.loaded(p) {
                        m.leash_pos = None;
                        continue;
                    }
                    (p.as_dvec3() + DVec3::new(0.5, 0.7, 0.5), !w.block(p).is_some_and(fence))
                }
            };
            let delta = anchor - (m.pos + DVec3::Y * m.shape().height * 0.5);
            let dist = delta.length();
            if broken || !m.alive() || dist > 10.0 {
                let pos = m.pos;
                m.leash = None;
                m.leash_pos = None;
                self.scatter(Stack::new(Item::LEAD, 1), pos);
                continue;
            }
            m.leash_pos = Some(anchor);
            if dist > 6.0 && !m.animal.as_ref().is_some_and(|a| a.sitting) {
                let n = delta / dist;
                m.vel += n * n.abs() * 0.4 * (dt as f64 * 20.0);
            }
        }
    }
}
impl Mob {
    pub(super) fn leash_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let at = self.leash_pos?;
        if self.animal.as_ref().is_some_and(|a| a.sitting) {
            return Some((None, 0.0));
        }
        let delta = (at - self.pos) * DVec3::new(1.0, 0.0, 1.0);
        if delta.length_squared() < 4.0 {
            return None;
        }
        self.look_at(at, 1.2);
        Some((Some(self.steer(delta.normalize_or(DVec3::X), dt, w, rng)), 2.5))
    }
}
pub(super) fn save(m: &Mob) -> Value {
    let leash = match m.leash {
        Some(Leash::Player(id)) => json!({"player":id.0}),
        Some(Leash::Fence(p)) => json!({"fence":p.to_array()}),
        None => Value::Null,
    };
    json!({"name":m.name.as_str(),"leash":leash})
}
pub(super) fn load(m: &mut Mob, v: &Value) {
    m.name = v["name"].as_str().and_then(StackName::new).unwrap_or_default();
    m.leash = v["leash"]["player"]
        .as_u64()
        .and_then(|id| u32::try_from(id).ok())
        .map(|id| Leash::Player(PlayerId(id)))
        .or_else(|| {
            let a = v["leash"]["fence"].as_array()?;
            if a.len() != 3 {
                return None;
            }
            Some(Leash::Fence(IVec3::new(
                i32::try_from(a[0].as_i64()?).ok()?,
                i32::try_from(a[1].as_i64()?).ok()?,
                i32::try_from(a[2].as_i64()?).ok()?,
            )))
        });
    if m.name.as_str().is_some() || m.leash.is_some() {
        m.persistent = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        physics::{BlockSource, test_util::Grid},
        world::terrain::Dimension,
    };
    struct Flat(Grid);
    impl BlockSource for Flat {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Flat {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
    }
    fn ctx(at: DVec3) -> Ctx {
        Ctx {
            players: vec![super::super::Target::new(PlayerId(3), at, false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    fn leads(e: &Entities) -> u8 {
        e.items.iter().filter(|i| i.stack.item == Item::LEAD).map(|i| i.stack.count).sum()
    }

    #[test]
    fn leads_attach_tie_to_fences_and_drop_when_the_knot_breaks() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(2.0, 1.0, 0.0));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3)), Some(true));
        assert_eq!(e.mobs[0].leash, Some(Leash::Player(PlayerId(3))));
        let post = IVec3::new(0, 1, 0);
        assert!(e.use_fence_knot(PlayerId(3), post));
        assert_eq!(e.mobs[0].leash, Some(Leash::Fence(post)));
        assert!(e.break_fence_knot(post));
        assert_eq!(e.mobs[0].leash, None);
        assert_eq!(leads(&e), 1);
    }

    #[test]
    fn leads_snap_beyond_ten_blocks_and_pull_within() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(0.0, 1.0, 0.0));
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        let w = Flat(Grid::flat(0));
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(8.0, 1.0, 0.0)));
        assert!(e.mobs[0].leash.is_some());
        assert!(e.mobs[0].vel.x > 0.0, "a taut lead pulls toward its holder");
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(14.0, 1.0, 0.0)));
        assert_eq!(e.mobs[0].leash, None);
        assert_eq!(leads(&e), 1);
    }

    #[test]
    fn hostile_mobs_cannot_be_leashed() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Zombie, DVec3::Y);
        assert_eq!(e.use_lead_or_name(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3)), None);
    }

    #[test]
    fn name_tags_need_a_name_and_names_survive_saves() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::Y);
        assert_eq!(e.use_lead_or_name(0, Some(Stack::new(Item::NAME_TAG, 1)), PlayerId(3)), None);
        let tag = Stack { name: StackName::new("Bessie").unwrap(), ..Stack::new(Item::NAME_TAG, 1) };
        assert_eq!(e.use_lead_or_name(0, Some(tag), PlayerId(3)), Some(true));
        assert!(e.mobs[0].persistent);
        e.mobs[0].leash = Some(Leash::Fence(IVec3::new(4, 5, -6)));
        let mut m = Mob::new(MobKind::Cow, DVec3::Y, 0.0);
        load(&mut m, &save(&e.mobs[0]));
        assert_eq!(m.name.as_str(), Some("Bessie"));
        assert_eq!(m.leash, Some(Leash::Fence(IVec3::new(4, 5, -6))));
        assert!(m.persistent);
    }
}
