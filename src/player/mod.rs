//! First-person player: Java's movement, collision and camera orientation.
//!
//! [`Player::tick`] runs one 20 Hz tick of Java 26.3's `LocalPlayer.aiStep`,
//! `LivingEntity.travel` and `Entity.move` (see [`travel`]). Velocity is
//! kept in blocks per second (Java's `deltaMovement` times 20).

mod fluid;
mod travel;

pub use fluid::{FluidCell, FluidContact, fluid_at};

use glam::{DVec3, IVec3, Vec3};

use crate::entity::model::PlayerPose;
use crate::physics::{self, Shape};
use crate::world::World;
use crate::world::block::Block;

pub const EYE_HEIGHT: f64 = PlayerPose::STANDING_EYE;
pub const HALF_WIDTH: f64 = PlayerPose::HALF_WIDTH;
pub const HEIGHT: f64 = PlayerPose::STANDING_HEIGHT;
/// The player's `STEP_HEIGHT` attribute (0.6), widened from Java's float.
pub const STEP_HEIGHT: f64 = 0.6_f32 as f64;
pub const SHAPE: Shape = Shape::new(HALF_WIDTH, HEIGHT);

/// One tick of held controls, as Java's `ClientInput` sees them.
#[derive(Default, Clone, Copy)]
pub struct MoveInput {
    /// Forward (+) or back (-), -1..=1.
    pub forward: f64,
    /// Right (+) or left (-), -1..=1.
    pub right: f64,
    pub jump: bool,
    /// The sneak key (Java's `isShiftKeyDown`): crouch, descend while flying,
    /// sink in water, and use a block's secondary action.
    pub descend: bool,
    /// The sprint key. Sprinting itself is the latched [`Player::sprinting`].
    pub sprint: bool,
    /// Using an item that slows the player (eating, drinking, drawing a bow):
    /// Java's 0.2 input factor and no sprinting.
    pub using_item: bool,
    /// Hunger at 6 or less (survival) or blindness: Java's
    /// `isSprintingPossible` is false and any sprint stops.
    pub sprint_blocked: bool,
}

pub struct Player {
    /// Distance-driven render animation, shared by host, controllers and CLI players.
    pub animation: crate::entity::player_model::WalkAnimation,
    /// Feet position (bottom centre of the bounding box).
    pub pos: DVec3,
    /// Blocks per second (Java's `deltaMovement` times 20).
    pub vel: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub flying: bool,
    /// Creative mode: flight allowed.
    pub can_fly: bool,
    /// Spectator mode: movement ignores block collision.
    pub noclip: bool,
    /// Touching water (Java `isInWater`).
    pub in_water: bool,
    /// Java Entity.shared flag 4: sprint-swim while submerged.
    pub swimming: bool,
    /// Java's latched sprint flag (`isSprinting`).
    pub sprinting: bool,
    /// Java `LocalPlayer.crouching`: slow, with the crouching pose.
    pub crouching: bool,
    /// Collision-resolved pose (Player.updatePlayerPose).
    pub pose: PlayerPose,
    /// HumanoidModel.swimAmount, 0..1.
    pub swim_amount: f32,
    /// Previous tick's eye-in-water (Entity.wasEyeInWater).
    pub(crate) eye_was_in_water: bool,
    /// Jumped off the ground during the last tick (hunger).
    pub jumped: bool,
    /// On a ladder, vine or scaffolding (`onClimbable`): falls are broken.
    pub climbing: bool,
    /// Stuck in a cobweb, berry bush or powder snow (`makeStuckInBlock`): falls are broken.
    pub in_web: bool,
    /// Java HoneyBlock.entityInside reset fall distance this tick.
    pub honey_sliding: bool,
    /// Landed on honey this tick (sound and ten block particles).
    pub honey_landed: bool,
    /// The sneak key is held (Java `isShiftKeyDown`).
    pub sneaking: bool,
    /// Java powder-snow contacts (previous contact is used for climbing).
    pub in_powder_snow: bool,
    pub was_in_powder_snow: bool,
    pub leather_boots: bool,
    pub freeze_immune: bool,
    /// Java `getPercentFrozen` for the frost `ADD_VALUE` speed modifier
    /// (`LivingEntity.tryAddFrost`); zero while standing on air.
    pub frozen_fraction: f32,
    /// Status effects on movement (see [`Player::apply_effects`]).
    modifiers: Modifiers,
    /// Minecart this player is riding.
    pub vehicle: Option<u32>,
    /// `GameRenderer`'s previous and current field-of-view modifier.
    fov: (f32, f32),
    /// Java movement state carried between ticks.
    motion: travel::Motion,
}

/// What status effects and equipment do to movement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Modifiers {
    dolphins_grace: bool,
    speed_level: u32,
    slowness_level: u32,
    jump_levels: u32,
    slow_falling: bool,
    /// Depth strider level of the worn boots (see [`Player::wear_boots`]).
    depth_strider: u8,
}

impl Player {
    pub fn powder_snow_equipment(&mut self, armor: &[Option<crate::inventory::Stack>; 4], ticks_frozen: u16) {
        use crate::item::{ArmorMaterial, ArmorPiece};
        self.leather_boots =
            armor[3].is_some_and(|s| s.item.as_armor() == Some((ArmorPiece::Boots, ArmorMaterial::Leather)));
        self.freeze_immune =
            armor.iter().flatten().any(|s| s.item.as_armor().is_some_and(|(_, m)| m == ArmorMaterial::Leather));
        self.frozen_fraction = f32::from(ticks_frozen.min(140)) / 140.0;
    }
    pub fn collision_context(&self) -> crate::physics::CollisionContext {
        crate::physics::CollisionContext {
            feet_y: self.pos.y,
            descending: self.sneaking,
            fall_distance: self.motion.fall_distance,
            leather_boots: self.leather_boots,
        }
    }
    pub fn new(pos: DVec3) -> Self {
        Self {
            animation: Default::default(),
            pos,
            vel: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            flying: false,
            can_fly: false,
            noclip: false,
            in_water: false,
            swimming: false,
            sprinting: false,
            crouching: false,
            pose: PlayerPose::Standing,
            swim_amount: 0.0,
            eye_was_in_water: false,
            jumped: false,
            climbing: false,
            in_web: false,
            honey_sliding: false,
            honey_landed: false,
            sneaking: false,
            in_powder_snow: false,
            was_in_powder_snow: false,
            leather_boots: false,
            freeze_immune: false,
            frozen_fraction: 0.0,
            modifiers: Modifiers::default(),
            vehicle: None,
            fov: (1.0, 1.0),
            motion: Default::default(),
        }
    }

    pub fn collision_shape(&self) -> Shape {
        self.pose.shape()
    }

    pub fn eye(&self) -> DVec3 {
        self.pos + DVec3::Y * self.pose.eye_height()
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cy * cp, sp, sy * cp)
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch - dy).clamp(-1.5533, 1.5533); // ±89°
    }

    /// Track the source of damage for Java's directional camera tilt.
    pub fn hurt_from(&mut self, push: DVec3) {
        if push.with_y(0.0).length_squared() > 1e-8 {
            self.animation.hurt_direction = (-push.z).atan2(-push.x) as f32 - self.yaw + std::f32::consts::FRAC_PI_2;
        }
    }

    /// Java's `isEyeInFluid(WATER)`: the eye is below the water surface in
    /// its cell (`getHeightForCamera`).
    pub fn head_in_water<W: physics::BlockSource + ?Sized>(&self, world: &W) -> bool {
        let eye = self.eye();
        let p = eye.floor().as_ivec3();
        world.block(p).and_then(fluid_at).is_some_and(|c| {
            c.fluid == crate::world::block::Fluid::Water
                && eye.y <= p.y as f64 + fluid::camera_height(world, p, c) as f64
        })
    }

    /// Whether any part of the player's box is in lava.
    pub fn in_lava(&self, world: &World) -> bool {
        physics::touches_block(world, self.pos, self.collision_shape(), Block::is_lava)
    }

    pub fn in_fire(&self, world: &World) -> bool {
        physics::touches_block(world, self.pos, self.collision_shape(), Block::is_fire)
    }

    pub fn in_soul_fire(&self, world: &World) -> bool {
        physics::touches_block(world, self.pos, self.collision_shape(), |b| {
            b == crate::world::nether_biome_blocks::SOUL_FIRE
        })
    }

    pub fn head_in_lava(&self, world: &World) -> bool {
        world.get_block(self.eye().floor().as_ivec3()).is_some_and(|b| b.is_lava())
    }

    /// Whether the player's box overlaps a block cell.
    /// Whether any cell the body overlaps holds `block`.
    pub fn touches(&self, world: &World, block: Block) -> bool {
        let shape = self.collision_shape();
        let (min, max) = shape.aabb(self.pos);
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
        (lo.y..=hi.y)
            .any(|y| (lo.z..=hi.z).any(|z| (lo.x..=hi.x).any(|x| world.get_block(IVec3::new(x, y, z)) == Some(block))))
    }

    pub fn intersects_block(&self, b: IVec3) -> bool {
        let bmin = b.as_dvec3();
        self.intersects_box(bmin, bmin + DVec3::ONE)
    }

    /// Whether the player's collision box overlaps the world box `bmin..bmax`.
    pub fn intersects_box(&self, bmin: DVec3, bmax: DVec3) -> bool {
        let (min, max) = self.collision_shape().aabb(self.pos);
        min.cmplt(bmax).all() && max.cmpgt(bmin).all()
    }

    /// Takes on the movement effects of Speed, Slowness, Jump Boost,
    /// Slow Falling and Dolphin's Grace; call before each tick.
    pub fn apply_effects(&mut self, effects: &crate::simulation::effects::Effects) {
        use crate::simulation::effects::Effect;
        let (speed_level, slowness_level) = effects.movement_speed_levels();
        self.modifiers = Modifiers {
            speed_level,
            slowness_level,
            dolphins_grace: effects.has(Effect::DolphinsGrace),
            jump_levels: effects.jump_boost(),
            slow_falling: effects.has(Effect::SlowFalling),
            depth_strider: self.modifiers.depth_strider,
        };
    }

    /// Depth Strider on the worn boots: Java's water movement efficiency of
    /// a third per level, halved off the ground.
    pub fn wear_boots(&mut self, depth_strider: u8) {
        self.modifiers.depth_strider = depth_strider.min(3);
    }

    /// Java's `MOVEMENT_SPEED` attribute: 0.1, with sprinting, Speed and Slowness.
    pub fn movement_speed(&self) -> f32 {
        crate::knockback::movement_speed_with(
            self.modifiers.speed_level,
            self.modifiers.slowness_level,
            self.sprinting,
            -0.05 * self.frozen_fraction,
        )
    }

    /// `AbstractClientPlayer.getFieldOfViewModifier` before the bow zoom:
    /// flying widens the view by a tenth, and speed by half its gain over walking.
    fn target_fov(&self) -> f32 {
        let flying = if self.flying { 1.1 } else { 1.0 };
        let modifier = flying * (self.movement_speed() / 0.1 + 1.0) / 2.0;
        if modifier.is_finite() { modifier } else { 1.0 }
    }

    /// `GameRenderer.tickFov`: the modifier moves halfway to its target each tick.
    fn tick_fov(&mut self) {
        let now = self.fov.1 + (self.target_fov() - self.fov.1) * 0.5;
        self.fov = (self.fov.1, now.clamp(0.1, 1.5));
    }

    /// The field-of-view modifier between the last two ticks (`alpha` 0..1).
    pub fn fov_modifier(&self, alpha: f32) -> f32 {
        self.fov.0 + (self.fov.1 - self.fov.0) * alpha.clamp(0.0, 1.0)
    }

    /// Blocks per tick a rider pushes a slow cart (Java's 0.1 of the walk wish).
    pub fn ride_push(&self, input: MoveInput) -> DVec3 {
        let yaw = self.yaw as f64;
        let fwd = DVec3::new(yaw.cos(), 0.0, yaw.sin());
        let right = DVec3::new(-yaw.sin(), 0.0, yaw.cos());
        let mut wish = fwd * input.forward + right * input.right;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }
        wish * 0.22
    }

    /// The Java state a tick carries to the next (jump delay, sprint
    /// double-tap window, last tick's sneak and forward keys), for a LAN
    /// client to replay inputs from the host's acknowledged tick.
    pub fn carried_motion(&self) -> [u8; 4] {
        let m = &self.motion;
        [m.no_jump_delay, m.sprint_trigger_time, m.was_shift as u8, m.had_forward as u8]
    }

    /// The cobweb, berry bush or powder snow slow-down the next move takes
    /// (Java's `stuckSpeedMultiplier`), carried like [`Player::carried_motion`].
    pub fn carried_stuck(&self) -> DVec3 {
        self.motion.stuck
    }

    /// Restores [`Player::carried_stuck`].
    pub fn restore_carried_stuck(&mut self, stuck: DVec3) {
        self.motion.stuck = stuck.clamp(DVec3::ZERO, DVec3::splat(2.0));
    }

    /// Restores [`Player::carried_motion`].
    pub fn restore_carried_motion(&mut self, carried: [u8; 4]) {
        let m = &mut self.motion;
        (m.no_jump_delay, m.sprint_trigger_time) = (carried[0].min(10), carried[1].min(7));
        (m.was_shift, m.had_forward) = (carried[2] != 0, carried[3] != 0);
    }

    /// Whether the last tick began touching lava (`Entity.baseTick`'s fluid
    /// contact, before the move).
    pub fn lava_before_move(&self) -> bool {
        self.motion.fluid.in_lava()
    }

    /// A riding player skips [`Player::tick`] but still runs Java's
    /// `baseTick` and block effects: refresh powder snow contact so it
    /// doesn't stay frozen on (or off) while the vehicle moves.
    pub fn refresh_riding_contact<W: physics::BlockSource + ?Sized>(&mut self, world: &W) {
        self.was_in_powder_snow = self.in_powder_snow;
        self.in_powder_snow = physics::touches_block(world, self.pos, self.collision_shape(), |b| {
            b == crate::world::scaffolding::POWDER_SNOW
        });
    }

    /// Whether a horizontal collision stopped the last tick's movement.
    pub fn horizontal_collision(&self) -> bool {
        self.motion.horizontal_collision
    }
}
