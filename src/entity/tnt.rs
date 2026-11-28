//! Primed TNT: a block-sized entity that falls, slides to a stop, flashes
//! and swells, then explodes when its fuse runs out.

use glam::DVec3;

use super::EntityEvent;
use super::trident::Owner;
use crate::physics::{self, BlockSource, Shape};
use crate::simulation::damage::{Cause, DamageType};

/// Seconds from lighting TNT to the blast (Minecraft's 80 ticks).
pub const FUSE: f32 = 4.0;
/// Blast strength (a creeper is 3).
pub const POWER: f32 = 4.0;
const GRAVITY: f64 = 20.0;
pub(crate) const SHAPE: Shape = Shape::new(0.49, 0.98);

pub struct PrimedTnt {
    /// Bottom centre.
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub(crate) vel: DVec3,
    /// Seconds left before it blows.
    pub fuse: f32,
    power: f32,
    /// v1.0 gap Track 8e: who lit it (`PrimedTnt.owner`): the player with
    /// flint and steel or a fire charge, a burning projectile's owner, or
    /// whoever was behind the blast that set it off; nobody for fire,
    /// redstone and dispensers. Saved as Java's `owner`.
    pub owner: Option<Owner>,
    /// v1.0 final F9: `Entity.fallDistance` (not saved), for powder snow.
    fall_distance: f64,
}

impl PrimedTnt {
    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(pos: DVec3, previous_pos: DVec3, fuse: f32) -> Self {
        Self { pos, previous_pos, vel: DVec3::ZERO, fuse, power: POWER, owner: None, fall_distance: 0.0 }
    }

    /// Create a moving charge with a fuse in seconds and coincident interpolation positions.
    pub fn new(pos: DVec3, vel: DVec3, fuse: f32) -> Self {
        Self { pos, previous_pos: pos, vel, fuse, power: POWER, owner: None, fall_distance: 0.0 }
    }

    /// v1.0 gap Track 8e: the level file's copy (`PrimedTnt`'s `fuse`,
    /// `explosion_power` and `owner`).
    pub(super) fn save(&self) -> serde_json::Value {
        use super::saved_projectiles::{owner_json, v3};
        serde_json::json!({
            "pos": v3(self.pos),
            "motion": v3(self.vel),
            "fuse": self.fuse,
            "power": self.power,
            "owner": owner_json(self.owner),
        })
    }

    pub(super) fn load(v: &serde_json::Value) -> Option<Self> {
        use super::saved_projectiles as s;
        let pos = s::read_v3(&v["pos"])?;
        Some(Self {
            pos,
            previous_pos: pos,
            vel: s::read_v3(&v["motion"]).unwrap_or(DVec3::ZERO),
            fuse: s::read_f32(v, "fuse", FUSE).clamp(0.0, 3276.7),
            // `Mth.clamp(explosion_power, 0, 128)`.
            power: s::read_f32(v, "power", POWER).clamp(0.0, 128.0),
            owner: s::read_owner(&v["owner"]),
            fall_distance: 0.0,
        })
    }

    /// Already detonating TNT cart; reuse the normal explosion event path.
    pub fn with_power(pos: DVec3, power: f32) -> Self {
        Self { power, ..Self::new(pos, DVec3::ZERO, 0.0) }
    }

    /// Falls and slides; returns `false` (after queueing the blast) once
    /// the fuse has burnt down. `alive` says whether a mob is in the level,
    /// for the owner the blast names ([`blast_cause`]).
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        events: &mut Vec<EntityEvent>,
        explodes: bool,
        alive: impl Fn(u32) -> Option<super::MobKind>,
    ) -> bool {
        self.fuse -= dt as f32;
        if self.fuse <= 0.0 {
            // Without `tnt_explodes` Java's `PrimedTnt.explode` does nothing
            // and the entity is simply discarded.
            if explodes {
                let (cause, credit_player) = blast_cause(self.owner, alive);
                events.push(EntityEvent::Explosion {
                    center: self.pos + DVec3::Y * SHAPE.height / 2.0,
                    power: self.power,
                    cause,
                    credit_player,
                });
            }
            return false;
        }
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let context = Some(physics::CollisionContext::entity(self.pos.y, self.fall_distance, false));
        let start_y = self.pos.y;
        let on_ground = physics::move_box_context(world, &mut self.pos, &mut self.vel, delta, SHAPE, context).on_ground;
        self.fall_distance = super::item::fallen(self.fall_distance, on_ground, start_y, self.pos.y);
        if on_ground {
            let k = (1.0 - dt * 8.0).max(0.0);
            self.vel.x *= k;
            self.vel.z *= k;
        }
        self.pos.y > -64.0
    }

    /// Flashing white for the model, every quarter second.
    pub fn flash(&self) -> bool {
        (self.fuse * 4.0) as i32 % 2 == 0
    }

    /// Edge length of the model: it swells just before the blast.
    pub fn size(&self) -> f32 {
        1.0 + (1.0 - self.fuse / 0.4).clamp(0.0, 1.0).powi(2) * 0.2
    }
}

/// v1.0 gap Track 8e: primed TNT's blast (`Explosion.getDefaultDamageSource`:
/// `explosion(tnt, tnt.getOwner())`). The owner counts only while it is in
/// the level (`EntityReference.getLivingEntity`), so TNT a creeper's blast
/// set off, or a mob that has since died, names no one: "blew up"
/// (`explosion`). With an owner the blast is `player_explosion`, "was blown
/// up by" them, and the mobs a player's TNT kills are theirs
/// (`credit_player`). `mob` gives the kind of a living mob by uid.
pub fn blast_cause(owner: Option<Owner>, mob: impl Fn(u32) -> Option<super::MobKind>) -> (Cause, bool) {
    match owner {
        Some(Owner::Player(_)) => (Cause { tnt: true, ..Cause::PLAYER_EXPLOSION }.by(owner), true),
        Some(Owner::Mob(uid)) => match mob(uid) {
            Some(kind) => {
                let text = super::crystal::blown_up_by(kind);
                (Cause { tnt: true, ..Cause::new(DamageType::PlayerExplosion, text) }.by(owner), false)
            }
            None => (Cause::TNT, false),
        },
        None => (Cause::TNT, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{MobKind, PlayerId};

    /// v1.0 final F9: primed TNT counts its fall (`Entity.fallDistance`),
    /// so after more than 2.5 blocks powder snow's 0.9 shape holds it for
    /// the tick it lands (which resets the count), and it sinks in after.
    #[test]
    fn primed_tnt_rests_a_tick_on_powder_snow_after_a_long_fall_then_sinks() {
        use crate::world::{World, block::Block, chunk::ChunkData, scaffolding::POWDER_SNOW, terrain::Generator};
        use glam::IVec3;
        use std::sync::Arc;
        let p = IVec3::new(8, 140, 8);
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, POWDER_SNOW);
        let fall = |from: f64| {
            let mut t = PrimedTnt::new(p.as_dvec3() + DVec3::new(0.5, from, 0.5), DVec3::ZERO, 100.0);
            let mut ys = Vec::new();
            for _ in 0..60 {
                t.update(0.05, &w, &mut Vec::new(), true, |_| None);
                ys.push(t.pos.y - p.y as f64);
            }
            ys
        };
        let snow_top = f64::from(0.9_f32);
        let long = fall(6.0);
        let rested = long.iter().filter(|&&y| (y - snow_top).abs() < 1e-6).count();
        assert_eq!(rested, 1, "one tick on the 0.9 shape: {long:?}");
        assert!(long.last().unwrap().abs() < 1e-6, "then down to the stone: {long:?}");
        let short = fall(1.5);
        assert!(short.iter().all(|&y| (y - snow_top).abs() > 1e-6), "a short fall never rests: {short:?}");
        assert!(short.last().unwrap().abs() < 1e-6);
    }

    #[test]
    fn the_blast_names_whoever_lit_the_tnt_while_they_are_in_the_level() {
        let none = |_| None;
        assert_eq!(blast_cause(None, none), (Cause::TNT, false));
        let (cause, credit) = blast_cause(Some(Owner::Player(PlayerId::HOST)), none);
        assert_eq!(
            (cause.kind, cause.text, cause.tnt, credit),
            (DamageType::PlayerExplosion, "was blown up by a player", true, true)
        );
        assert_eq!(cause.by, Some(Owner::Player(PlayerId::HOST)));
        let skeleton = |uid| (uid == 7).then_some(MobKind::Skeleton);
        let (cause, credit) = blast_cause(Some(Owner::Mob(7)), skeleton);
        assert_eq!(
            (cause.kind, cause.text, cause.tnt, credit),
            (DamageType::PlayerExplosion, "was blown up by a skeleton", true, false)
        );
        // The owner is gone (a creeper is discarded after its blast).
        assert_eq!(blast_cause(Some(Owner::Mob(8)), skeleton), (Cause::TNT, false));
        assert_eq!(crate::entity::crystal::blown_up_by(MobKind::Enderman), "was blown up by an enderman");
        assert_eq!(crate::entity::crystal::blown_up_by(MobKind::Zombie), "was blown up by a zombie");
        assert!(std::ptr::eq(
            crate::entity::crystal::blown_up_by(MobKind::Zombie),
            crate::entity::crystal::blown_up_by(MobKind::Zombie)
        ));
    }
}
