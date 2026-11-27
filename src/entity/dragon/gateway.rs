//! v1.0 gap-closing Track 4 (GEN-04b): End gateways' block entities, as
//! Java 26.3's `TheEndGatewayBlockEntity` keeps them.
//!
//! - **Age.** Each gateway counts the ticks since it was placed (`Age`,
//!   saved). For its first 200 (`isSpawning`) it shows a magenta beam up
//!   and down to the height of the world.
//! - **Cooldown.** Anything that uses a gateway (`EndGatewayBlock.entityInside`
//!   for a player or a thrown pearl) sets its `teleportCooldown` to 40
//!   (`triggerCooldown`), and every 2400 ticks of age it does so by itself
//!   (the `ATTENTION_INTERVAL` flash). While cooling down nothing can use
//!   it, and it shows a purple beam 50 blocks each way.
//! - **Beams** (`TheEndGatewayRenderer`): the scale is the spawn or cooldown
//!   percent through `sin(x π)`, the height `floor(scale × (max y or 50))`,
//!   from `-height` to `+height` around the block; radii 0.15 and 0.175.
//!
//! Gateways tick only while their chunk is loaded (`portalTick` is a block
//! entity ticker). VoxelCraft's End fight knows every gateway (the twenty
//! a kill opens and the exits built for them), so their block entities live
//! with the fight and save in its record.

use glam::IVec3;

use crate::physics::BlockSource;
use crate::world::block::Block;

/// `SPAWN_TIME`.
pub const SPAWN_TICKS: u64 = 200;
/// `COOLDOWN_TIME`.
pub const COOLDOWN_TICKS: u32 = 40;
/// `ATTENTION_INTERVAL`.
pub const ATTENTION_TICKS: u64 = 2400;
/// `TheEndGatewayRenderer`'s beam for a cooling gateway reaches 50 blocks.
const COOLDOWN_BEAM: f32 = 50.0;

/// One gateway's block entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gateway {
    pub pos: IVec3,
    /// `age`: ticks since it was placed.
    pub age: u64,
    /// `teleportCooldown`, in ticks.
    pub cooldown: u32,
}

impl Gateway {
    /// `isSpawning`.
    pub fn spawning(&self) -> bool {
        self.age < SPAWN_TICKS
    }

    /// `isCoolingDown`.
    pub fn cooling(&self) -> bool {
        self.cooldown > 0
    }

    /// `portalTick`.
    fn tick(&mut self) {
        let cooling = self.cooling();
        self.age += 1;
        if cooling {
            self.cooldown -= 1;
        } else if self.age.is_multiple_of(ATTENTION_TICKS) {
            self.cooldown = COOLDOWN_TICKS;
        }
    }

    /// The beam `TheEndGatewayRenderer` draws, `partial` of a tick on:
    /// its scale, its height each way from the block, and whether it is
    /// the spawn beam (magenta) rather than the cooldown beam (purple).
    pub fn beam(&self, partial: f32, max_y: i32) -> Option<(f32, i32, bool)> {
        let spawning = self.spawning();
        if !spawning && !self.cooling() {
            return None;
        }
        let percent = if spawning {
            ((self.age as f32 + partial) / SPAWN_TICKS as f32).clamp(0.0, 1.0)
        } else {
            1.0 - ((self.cooldown as f32 - partial) / COOLDOWN_TICKS as f32).clamp(0.0, 1.0)
        };
        let reach = if spawning { max_y as f32 } else { COOLDOWN_BEAM };
        let scale = crate::knockback::mth_sin(f64::from(percent * std::f32::consts::PI));
        let height = (scale * reach).floor() as i32;
        (height > 0).then_some((scale, height, spawning))
    }
}

/// The End's gateways.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gateways {
    pub list: Vec<Gateway>,
}

impl Gateways {
    /// A gateway was placed at `pos` (a new block entity, age 0); one
    /// already known keeps its state.
    pub fn placed(&mut self, pos: IVec3) {
        if self.get(pos).is_none() {
            self.list.push(Gateway { pos, age: 0, cooldown: 0 });
        }
    }

    /// A gateway from a save written before gateways kept their age: past
    /// its spawn beam.
    pub fn known(&mut self, pos: IVec3) {
        if self.get(pos).is_none() {
            self.list.push(Gateway { pos, age: SPAWN_TICKS, cooldown: 0 });
        }
    }

    pub fn get(&self, pos: IVec3) -> Option<&Gateway> {
        self.list.iter().find(|g| g.pos == pos)
    }

    /// `EndGatewayBlock.entityInside`'s check: the gateway at `pos` can't
    /// be used while it cools down.
    pub fn cooling(&self, pos: IVec3) -> bool {
        self.get(pos).is_some_and(Gateway::cooling)
    }

    /// `triggerCooldown`: something used the gateway at `pos`.
    pub fn trigger(&mut self, pos: IVec3) {
        match self.list.iter_mut().find(|g| g.pos == pos) {
            Some(g) => g.cooldown = COOLDOWN_TICKS,
            None => self.list.push(Gateway { pos, age: SPAWN_TICKS, cooldown: COOLDOWN_TICKS }),
        }
    }

    /// One tick of every gateway whose chunk is loaded.
    pub fn tick<W: BlockSource + ?Sized>(&mut self, world: &W) {
        for g in &mut self.list {
            if world.block(g.pos) == Some(Block::END_GATEWAY) {
                g.tick();
            }
        }
    }

    /// The save sections: `g x y z age cooldown` per gateway.
    pub fn serialize(&self) -> String {
        self.list.iter().map(|g| format!(";g {} {} {} {} {}", g.pos.x, g.pos.y, g.pos.z, g.age, g.cooldown)).collect()
    }

    /// Reads one save section, if it is a gateway's.
    pub fn load_section(&mut self, section: &str) -> bool {
        let Some(rest) = section.strip_prefix("g ") else { return false };
        let n: Vec<i64> = rest.split(' ').filter_map(|v| v.parse().ok()).collect();
        if let [x, y, z, age, cooldown] = n[..]
            && let (Ok(x), Ok(y), Ok(z)) = (i32::try_from(x), i32::try_from(y), i32::try_from(z))
            && self.get(IVec3::new(x, y, z)).is_none()
        {
            self.list.push(Gateway {
                pos: IVec3::new(x, y, z),
                age: age.max(0) as u64,
                cooldown: cooldown.clamp(0, i64::from(COOLDOWN_TICKS)) as u32,
            });
        }
        true
    }

    /// For a LAN client: `[x, y, z, age, cooldown]` per gateway.
    pub fn replica_json(&self) -> serde_json::Value {
        self.list.iter().map(|g| serde_json::json!([g.pos.x, g.pos.y, g.pos.z, g.age, g.cooldown])).collect()
    }

    pub fn apply_replica(&mut self, v: &serde_json::Value) {
        self.list = v
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .take(256)
            .filter_map(|g| {
                let n = |i: usize| g.get(i).and_then(serde_json::Value::as_i64);
                Some(Gateway {
                    pos: IVec3::new(n(0)? as i32, n(1)? as i32, n(2)? as i32),
                    age: n(3)?.max(0) as u64,
                    cooldown: n(4)?.clamp(0, i64::from(COOLDOWN_TICKS)) as u32,
                })
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct One(IVec3);
    impl BlockSource for One {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p == self.0 { Block::END_GATEWAY } else { Block::AIR })
        }
    }

    #[test]
    fn a_new_gateway_beams_for_200_ticks_then_flashes_every_2400() {
        let pos = IVec3::new(96, 75, 0);
        let world = One(pos);
        let mut g = Gateways::default();
        g.placed(pos);
        let beam = |g: &Gateways| g.get(pos).unwrap().beam(0.0, 256);
        assert!(beam(&g).is_none(), "age 0: sin(0) is no height");
        for _ in 0..100 {
            g.tick(&world);
        }
        // Half way: sin(π/2) = 1, the whole world's height each way, magenta.
        assert_eq!(beam(&g), Some((1.0, 256, true)));
        for _ in 100..200 {
            g.tick(&world);
        }
        assert!(!g.get(pos).unwrap().spawning());
        assert!(beam(&g).is_none());
        // Age 2400: the attention flash, a 40-tick purple beam of up to 50.
        for _ in 200..2399 {
            g.tick(&world);
        }
        assert!(!g.cooling(pos));
        g.tick(&world);
        assert!(g.cooling(pos));
        assert_eq!(g.get(pos).unwrap().cooldown, COOLDOWN_TICKS);
        for _ in 0..20 {
            g.tick(&world);
        }
        assert_eq!(beam(&g), Some((1.0, 50, false)));
        for _ in 0..20 {
            g.tick(&world);
        }
        assert!(!g.cooling(pos));
        // Not ticked where its chunk isn't loaded or the block is gone.
        let age = g.get(pos).unwrap().age;
        g.tick(&One(IVec3::ZERO));
        assert_eq!(g.get(pos).unwrap().age, age);
    }

    #[test]
    fn use_triggers_a_40_tick_cooldown_and_saves() {
        let pos = IVec3::new(-1, 75, 96);
        let mut g = Gateways::default();
        g.known(pos);
        assert!(!g.cooling(pos));
        g.trigger(pos);
        assert!(g.cooling(pos));
        let world = One(pos);
        for _ in 0..39 {
            g.tick(&world);
        }
        assert!(g.cooling(pos));
        let mut loaded = Gateways::default();
        for s in g.serialize().split(';').filter(|s| !s.is_empty()) {
            assert!(loaded.load_section(s));
        }
        assert_eq!(loaded, g);
        g.tick(&world);
        assert!(!g.cooling(pos));
        let mut replica = Gateways::default();
        replica.apply_replica(&g.replica_json());
        assert_eq!(replica, g);
    }
}
