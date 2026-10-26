//! Java 26.3 `SlimeBlock`: `stepOn` slows a walker on the block, and
//! `fallOn` (see `survival::Env::on_slime`) cancels fall damage, sneaking or not. The bounce
//! itself is `bounceRestitution(1.0F)` in `restitute`, and the 0.8 friction
//! is in `travel::friction`.
use super::Player;
use crate::physics::BlockSource;
use crate::world::stone_blocks::SLIME_BLOCK;
use glam::DVec3;

impl Player {
    /// `SlimeBlock.stepOn` through `Entity.applyEffectsFromBlocks`: on the
    /// ground and not sneaking, a nearly level walk is scaled by
    /// `0.4 + |dy| * 0.2` horizontally.
    pub(super) fn slime_step_on<W: BlockSource + ?Sized>(&self, world: &W, dm: &mut DVec3) {
        if !self.on_slime(world) || self.sneaking {
            return;
        }
        let dy = dm.y.abs();
        if dy < 0.1 {
            let scale = 0.4 + dy * 0.2;
            dm.x *= scale;
            dm.z *= scale;
        }
    }

    /// Standing on a slime block (`getOnPosLegacy`, as honey's check).
    pub fn on_slime<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        self.on_ground && world.block(self.on_pos(world, 0.2)) == Some(SLIME_BLOCK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
    use crate::player::MoveInput;
    use crate::player::travel::{bounciness, friction};
    use glam::IVec3;

    const FLOOR: i32 = 64;

    fn slime_floor() -> Grid {
        let mut world = Grid::flat(FLOOR);
        for x in -4..40 {
            for z in -4..5 {
                world.set(IVec3::new(x, FLOOR - 1, z), SLIME_BLOCK);
            }
        }
        world
    }

    #[test]
    fn slime_block_friction_and_restitution_follow_blocks_java() {
        assert_eq!(friction(Some(SLIME_BLOCK)), 0.8);
        assert_eq!(bounciness(Some(SLIME_BLOCK)), 1.0);
        assert_eq!(friction(Some(crate::world::block::Block::STONE)), 0.6);
    }

    #[test]
    fn a_fall_onto_slime_bounces_back_unless_sneaking() {
        let world = slime_floor();
        let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 6.0, 0.5));
        let mut peak_after_bounce = f64::MIN;
        let mut bounced = false;
        for _ in 0..80 {
            let was_falling = p.vel.y < 0.0;
            p.tick(MoveInput::default(), &world);
            bounced |= was_falling && p.vel.y > 0.0;
            if bounced {
                peak_after_bounce = peak_after_bounce.max(p.pos.y);
            }
        }
        assert!(bounced);
        // Restitution 1.0 returns most of the height (air drag takes the rest).
        assert!(peak_after_bounce > FLOOR as f64 + 4.0, "{peak_after_bounce}");

        let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 6.0, 0.5));
        let sneak = MoveInput { descend: true, ..Default::default() };
        for _ in 0..80 {
            p.tick(sneak, &world);
            assert!(p.vel.y <= 0.0, "no bounce while sneaking");
        }
    }

    #[test]
    fn walking_on_slime_is_slower_than_on_stone() {
        let walk = |world: &Grid| {
            let mut p = Player::new(DVec3::new(0.5, FLOOR as f64, 0.5));
            for _ in 0..3 {
                p.tick(MoveInput::default(), world);
            }
            let start = p.pos.x;
            for _ in 0..100 {
                p.tick(MoveInput { forward: 1.0, ..Default::default() }, world);
            }
            p.pos.x - start
        };
        let on_stone = walk(&Grid::flat(FLOOR));
        let on_slime = walk(&slime_floor());
        assert!(on_slime < on_stone * 0.6, "slime {on_slime} vs stone {on_stone}");
        assert!(on_slime > 0.0);
    }
}
