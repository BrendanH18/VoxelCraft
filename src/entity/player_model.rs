//! Classic 64x64 humanoid skin layout and Java's distance-driven limb animation.
//! Model pixels use +Y up, +Z forward (Java's Y/Z signs are reversed).
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{DVec3, Mat4, Quat, Vec3};

use super::model::{EntityVertex, cube, push_cuboid_flat};
use crate::entity::model::PlayerPose;
use crate::item::Item;
use crate::player::Player;

/// Default block friction (0.6) times the 0.91 air drag Java applies to
/// `deltaMovement` at the end of a ground `travel`.

#[derive(Default, Debug)]
pub struct WalkAnimation {
    pub position: f32,
    pub speed: f32,
    pub previous_speed: f32,
    pub body_yaw: f32,
    pub walk_dist: f32,
    pub previous_walk_dist: f32,
    pub bob: f32,
    pub previous_bob: f32,
    /// Direction of the latest knockback in the Java camera frame, radians.
    pub hurt_direction: f32,
    initialized: bool,
}

impl WalkAnimation {
    /// LivingEntity.calculateEntityAnimation: displacement * 4, capped at 1;
    /// WalkAnimationState smooths speed by 0.4 per 20 Hz tick, then accumulates it.
    pub fn update(&mut self, distance: f32, dt: f32, yaw: f32, flying: bool) {
        let ticks = dt * 20.0;
        self.previous_walk_dist = self.walk_dist;
        self.walk_dist += distance * 0.6;
        self.previous_speed = self.speed;
        let target = if flying { 0.0 } else { (distance * 4.0 / ticks.max(1e-4)).min(1.0) };
        self.speed += (target - self.speed) * (1.0 - 0.6f32.powf(ticks));
        self.position += self.speed * ticks;
        if !self.initialized {
            self.body_yaw = yaw;
            self.initialized = true;
        }
        let delta = angle(yaw - self.body_yaw);
        self.body_yaw += delta * (1.0 - 0.7f32.powf(ticks));
        // Java's body/head limit keeps the head from looking through its back.
        self.body_yaw = yaw - angle(yaw - self.body_yaw).clamp(-85f32.to_radians(), 85f32.to_radians());
    }

    /// v1.0 Track 9: `ClientAvatarState.updateBob`, from
    /// `AbstractClientPlayer.updateBob` once a tick: `target` is
    /// min(0.1, the horizontal `deltaMovement`) on the ground, alive and not
    /// swimming, otherwise 0.
    pub fn update_bob(&mut self, target: f32) {
        self.previous_bob = self.bob;
        self.bob += (target - self.bob) * 0.4;
    }

    /// `ClientAvatarState.resetBob`: riding stops the bob at once.
    pub fn reset_bob(&mut self) {
        self.previous_bob = self.bob;
        self.bob = 0.0;
    }

    pub fn sample(&self, alpha: f32) -> (f32, f32) {
        (self.position - self.speed * (1.0 - alpha), self.previous_speed + (self.speed - self.previous_speed) * alpha)
    }
}

fn angle(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Java's `HumanoidModel.ArmPose` for ranged weapons.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub enum Aim {
    #[default]
    None,
    /// A bow being drawn: both arms raised toward the target.
    Bow,
    /// A crossbow being loaded, 0..1 of the way.
    CrossbowCharge(f32),
    /// A loaded crossbow held out to fire.
    CrossbowHold,
    /// v1.0 gap track 6e: a trident being charged (`THROW_TRIDENT`): the
    /// arm raised behind the head and the trident's `item/trident_throwing`
    /// display.
    Trident,
}

/// Render inputs belong to the view/player, never to a global camera.
#[derive(Clone, Copy, Default)]
pub struct PlayerAppearance {
    pub held: Option<Item>,
    pub swing: f32,
    pub using: f32,
    pub hurt: bool,
    /// Skin is hidden; equipped armor and the held item remain visible.
    pub invisible: bool,
    /// Seconds since death; absent for a living player.
    pub death: Option<f32>,
    pub alpha: f32,
    /// Helmet, chest, leggings, boots.
    pub armor: [Option<super::armor::Worn>; 4],
    /// Animated compass or clock layer, when `held` is one of those.
    pub held_icon: Option<u16>,
    /// Pose-bearing statue block model from the complete held stack.
    pub held_statue: Option<crate::world::block::Block>,
    /// The held stack's `minecraft:dye` icon tint (a dyed or leather item);
    /// `None` falls back to the item's undyed tint.
    pub held_tint: Option<[u8; 3]>,
    /// v1.0 gap track 6c: the whole held stack, when known (its glint,
    /// banner patterns and 3D model); `None` draws `held` as a plain stack.
    pub held_stack: Option<crate::inventory::Stack>,
    /// v1.0 final track F1: the off-hand stack, held in the left hand
    /// (`ItemInHandLayer` with `HumanoidArm.LEFT`).
    pub off_stack: Option<crate::inventory::Stack>,
    pub aim: Aim,
    /// v0.15 Track D: under Glowing, outlined through walls.
    pub glowing: bool,
    /// v0.16 Track B: an elytra is worn (its wings are drawn).
    pub elytra: bool,
    /// v1.0 gap Track 8d: on fire (`displayFireAnimation`, which a
    /// spectator never is): Java's flame is drawn over the player's box.
    pub burning: bool,
    /// The camera's look, which the flame faces (`Vec3::ZERO`: toward the
    /// camera's position instead).
    pub view: Vec3,
}

const PLAYER_SCALE: f32 = 0.9375;
const UNTINTED: [u8; 4] = [255, 255, 255, 0];

/// v1.0 gap track 6c: `ItemInHandLayer`'s right-hand grip on an arm with
/// `pivot` and `rot` (in this model's frame, pixels), as a matrix from the
/// held item's model space (a block is 0..1) to camera-relative blocks,
/// before the item's display transform: `translateToHand`, then a turn of
/// -90 degrees about x and 180 about y and Java's offset of (1, 2, -10)
/// sixteenths, or (0, 1, -4.5) for a baby model (`useBabyOffset`). Java's
/// model frame is this one turned half about x (y down, back toward +Z).
pub(crate) fn third_person_hand(pivot: Vec3, rot: Quat, origin: Vec3, body: Quat, scale: f32, baby: bool) -> Mat4 {
    let offset = if baby { Vec3::new(0.0, 1.0, -4.5) } else { Vec3::new(1.0, 2.0, -10.0) };
    Mat4::from_translation(origin)
        * Mat4::from_quat(body)
        * Mat4::from_scale(Vec3::splat(scale / 16.0))
        * Mat4::from_translation(pivot)
        * Mat4::from_quat(rot)
        * Mat4::from_rotation_x(PI)
        * Mat4::from_scale(Vec3::splat(16.0))
        * Mat4::from_rotation_x(-FRAC_PI_2)
        * Mat4::from_rotation_y(PI)
        * Mat4::from_translation(offset / 16.0)
}

/// v1.0 gap track 6d: `ItemInHandLayer`'s chain after a model's
/// `translateToHand`, given that hand as a matrix from this model's frame
/// (pixels) to camera-relative blocks: half a turn about x into Java's
/// frame, -90 degrees about x, 180 about y and the adult offset of (1, 2,
/// -10) sixteenths, from the held item's model space.
pub(crate) fn item_in_hand_layer(hand: Mat4) -> Mat4 {
    hand * Mat4::from_rotation_x(PI)
        * Mat4::from_scale(Vec3::splat(16.0))
        * Mat4::from_rotation_x(-FRAC_PI_2)
        * Mat4::from_rotation_y(PI)
        * Mat4::from_translation(Vec3::new(1.0, 2.0, -10.0) / 16.0)
}

/// v1.0 gap track 6c: `stack` held in the third-person right hand `hand`
/// ([`third_person_hand`]): its model placed with Java's
/// `thirdperson_righthand` display transform, for the renderer's item model
/// pass to draw with the glint and translucency (`light` is the holder's
/// sky and block light, 0..1).
pub(crate) fn held_item(
    stack: crate::inventory::Stack,
    hand: Mat4,
    light: (f32, f32),
    block: Option<crate::world::block::Block>,
    icon: Option<u16>,
    tint: Option<[u8; 3]>,
) -> super::model::HeldItemModel {
    let display = crate::content::item_display::held(&stack);
    let transform = hand * crate::content::item_display::Context::ThirdPersonRightHand.matrix(&display);
    super::model::HeldItemModel { stack, transform, sky_light: light.0, block_light: light.1, block, icon, tint }
}

#[derive(Clone, Copy)]
pub(crate) struct Limb {
    pub pivot: Vec3,
    pub rot: Quat,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub uv: [f32; 2],
}

fn limb(pivot: [f32; 3], min: [f32; 3], max: [f32; 3], uv: [f32; 2], rot: Quat) -> Limb {
    Limb { pivot: Vec3::from_array(pivot), min, max, uv, rot }
}

/// Head, torso, right arm, left arm, right leg, left leg. Overlays/armor
/// copy these transforms so they cannot lag behind an attack or sneak pose.
pub(super) fn pose(player: &Player, appearance: &PlayerAppearance, time: f32) -> [Limb; 6] {
    let (position, mut amount) = player.animation.sample(appearance.alpha);
    // v0.16 Track B: `speedValue` damps the limbs of a fast glide.
    if player.fall_flying {
        let speed = (player.vel / 20.0).length_squared() as f32 / 0.2;
        amount /= (speed * speed * speed).max(1.0);
    }
    let phase = position * 0.6662;
    let walk_right = (phase + PI).cos() * amount;
    let mut right_arm = walk_right;
    let mut left_arm = phase.cos() * amount;
    if appearance.held.is_some() {
        right_arm = right_arm * 0.5 - PI / 10.0;
    }
    if appearance.using > 0.0 {
        right_arm = -FRAC_PI_2 - player.pitch;
    }
    // Java's ranged arm poses (`HumanoidModel.poseRightArm` and `AnimationUtils`),
    // with its Y rotations negated for this model's flipped axes.
    let head_yaw = angle(player.animation.body_yaw - player.yaw);
    let (mut right_yaw, mut left_yaw) = (None, None);
    match appearance.aim {
        Aim::None => {}
        Aim::Bow => {
            (right_arm, left_arm) = (-FRAC_PI_2 - player.pitch, -FRAC_PI_2 - player.pitch);
            (right_yaw, left_yaw) = (Some(0.1 + head_yaw), Some(-0.5 + head_yaw));
        }
        Aim::CrossbowCharge(done) => {
            right_arm = -0.97079635;
            left_arm = -0.97079635 + (-FRAC_PI_2 + 0.97079635) * done;
            (right_yaw, left_yaw) = (Some(0.8), Some(-(0.4 + (0.85 - 0.4) * done)));
        }
        Aim::CrossbowHold => {
            (right_arm, left_arm) = (-FRAC_PI_2 - player.pitch + 0.1, -1.5 - player.pitch);
            (right_yaw, left_yaw) = (Some(0.3 + head_yaw), Some(-0.6 + head_yaw));
        }
        // `HumanoidModel.poseRightArm`'s `THROW_TRIDENT`, in place of the
        // held item's pose: half the walk swing, a half turn back.
        Aim::Trident => {
            right_arm = walk_right * 0.5 - PI;
            right_yaw = Some(0.0);
        }
    }
    let attack = appearance.swing.clamp(0.0, 1.0);
    let turn = (attack.sqrt() * TAU).sin() * 0.2;
    let ease = 1.0 - (1.0 - attack).powi(4);
    right_arm -= (ease * PI).sin() * 1.2 + (attack * PI).sin() * (player.pitch + 0.7) * 0.75;
    left_arm += turn;
    let riding = player.vehicle.is_some();
    if riding {
        right_arm -= std::f32::consts::PI / 5.;
        left_arm -= std::f32::consts::PI / 5.;
    }
    let swim = if riding { 0. } else { player.swim_amount };
    let crouch = !riding && player.pose == PlayerPose::Crouching;
    if crouch {
        right_arm += 0.4;
        left_arm += 0.4;
    }
    if swim > 0.0 {
        let cycle = (time * 20.0) % 26.0;
        let stroke = if cycle < 14.0 {
            (cycle / 14.0).powi(2)
        } else if cycle < 22.0 {
            (cycle - 14.0) / 8.0
        } else {
            1.0 - (cycle - 22.0) / 4.0
        };
        let blend = |from: f32, to: f32| from + (to - from) * swim;
        right_arm = blend(right_arm, FRAC_PI_2 * stroke);
        left_arm = blend(left_arm, FRAC_PI_2 * stroke);
        right_arm = blend(right_arm, 0.3 * (phase + PI).cos());
        left_arm = blend(left_arm, 0.3 * phase.cos());
    }
    let ticks = time * 20.0;
    let idle_x = (ticks * 0.067).sin() * 0.05;
    let idle_z = (ticks * 0.09).cos() * 0.05 + 0.05;
    let arm_y = if crouch { 18.8 } else { 22.0 };
    let head_y = if crouch { 19.8 } else { 24.0 };
    let (leg_y, leg_z) = if swim > 0.5 {
        (12.0, 0.0)
    } else if crouch {
        (11.8, -4.0)
    } else {
        (12.0, 0.0)
    };
    let body_tilt = if swim > 0.5 { -FRAC_PI_2 + player.pitch } else { 0.0 };
    let rx = Quat::from_rotation_x;
    let ry = Quat::from_rotation_y;
    let rz = Quat::from_rotation_z;
    [
        limb(
            [0.0, head_y, 0.0],
            [-4.0, 0.0, -4.0],
            [4.0, 8.0, 4.0],
            [0.0, 0.0],
            // v0.16 Track B: a gliding head looks 45 degrees up from the body.
            ry(angle(player.animation.body_yaw - player.yaw))
                * rx(if player.fall_flying { -std::f32::consts::FRAC_PI_4 } else { -player.pitch }),
        ),
        limb(
            [0.0, if crouch { 20.8 } else { 24.0 }, 0.0],
            [-4.0, -12.0, -2.0],
            [4.0, 0.0, 2.0],
            [16.0, 16.0],
            ry(turn) * rx(if crouch { 0.5 } else { 0.0 } + body_tilt),
        ),
        limb(
            [-turn.cos() * 5.0, arm_y, -turn.sin() * 5.0],
            [-3.0, -10.0, -2.0],
            [1.0, 2.0, 2.0],
            [40.0, 16.0],
            ry(right_yaw.unwrap_or(turn * 3.0)) * rx(right_arm + idle_x) * rz(-(attack * PI).sin() * 0.4 + idle_z),
        ),
        limb(
            [turn.cos() * 5.0, arm_y, turn.sin() * 5.0],
            [-1.0, -10.0, -2.0],
            [3.0, 2.0, 2.0],
            [32.0, 48.0],
            ry(left_yaw.unwrap_or(turn)) * rx(left_arm - idle_x) * rz(-idle_z),
        ),
        limb(
            [-1.9, leg_y, leg_z],
            [-2.0, -12.0, -2.0],
            [2.0, 0.0, 2.0],
            [0.0, 16.0],
            rx(if riding { -1.4137167 } else { phase.cos() * 1.4 * amount })
                * ry(if riding { std::f32::consts::PI / 10. } else { 0.005 })
                * rz(if riding { 0.07853982 } else { 0.005 }),
        ),
        limb(
            [1.9, leg_y, leg_z],
            [-2.0, -12.0, -2.0],
            [2.0, 0.0, 2.0],
            [16.0, 48.0],
            rx(if riding { -1.4137167 } else { (phase + PI).cos() * 1.4 * amount })
                * ry(if riding { -std::f32::consts::PI / 10. } else { -0.005 })
                * rz(if riding { -0.07853982 } else { -0.005 }),
        ),
    ]
}

/// One texture-mapped box, as triangles (its faces are painted per vertex below).
/// `material`: 1 = procedural skin atlas; 2 = existing block/item atlas.
#[allow(clippy::too_many_arguments)]
pub(crate) fn textured_box(
    out: &mut Vec<EntityVertex>,
    p: &Limb,
    inflation: f32,
    origin: Vec3,
    body: Quat,
    light: ([u8; 4], u8),
    material: u8,
    layer: u16,
    scale: f32,
    tint: [u8; 4],
) {
    let c = cube(p.min.map(|v| v - inflation), p.max.map(|v| v + inflation), [255; 3], 0);
    let start = out.len();
    push_cuboid_flat(
        out,
        &c,
        &|v| origin + body * (p.pivot + p.rot * v) * (scale / 16.0),
        body * p.rot,
        light,
        ([0.0; 3], 0.0),
        0.0,
    );
    let [w, h, d] = std::array::from_fn(|i| p.max[i] - p.min[i]);
    let [u, v] = p.uv;
    // Java's cuboid UV net: right/front/left/back around a top/bottom strip.
    let rects = [
        [u + d + w, v + d, d, h],
        [u, v + d, d, h],
        [u + d, v, w, d],
        [u + d + w, v, w, d],
        [u + d + w + d, v + d, w, h],
        [u + d, v + d, w, h],
    ];
    for (face, vertices) in out[start..].as_chunks_mut::<6>().0.iter_mut().enumerate() {
        let axis = face / 2;
        let positive = face % 2 == 1;
        let order = if positive {
            [(0., 0.), (1., 0.), (1., 1.), (0., 0.), (1., 1.), (0., 1.)]
        } else {
            [(0., 0.), (1., 1.), (1., 0.), (0., 0.), (0., 1.), (1., 1.)]
        };
        // rects use -X,+X,+Y,-Y,-Z,+Z order.
        let rect = rects[match axis {
            0 => usize::from(positive),
            1 => {
                if positive {
                    2
                } else {
                    3
                }
            }
            _ => {
                if positive {
                    5
                } else {
                    4
                }
            }
        }];
        for (vertex, (a, b)) in vertices.iter_mut().zip(order) {
            let (x, y) = match axis {
                0 => (b, 1.0 - a),
                1 => (a, b),
                _ => (if positive { a } else { 1.0 - a }, 1.0 - b),
            };
            vertex.uv = if material == 2 { [x, y] } else { [rect[0] + x * rect[2], rect[1] + y * rect[3]] };
            vertex.color = tint;
            vertex.torch[1] = material;
            vertex.torch[2] = layer as u8;
            vertex.torch[3] = (layer >> 8) as u8;
        }
    }
}

pub fn build_player(
    player: &Player,
    position: DVec3,
    camera: DVec3,
    lighting: (f32, f32),
    time: f32,
    appearance: PlayerAppearance,
    out: &mut super::model::EntityMesh,
) {
    let origin = (position - camera).as_vec3();
    let fall = appearance.death.map_or(0.0, |t| (((t * 20.0 - 1.0).max(0.0) / 20.0 * 1.6).sqrt()).min(1.0) * FRAC_PI_2);
    let mut body = Quat::from_rotation_y(FRAC_PI_2 - player.animation.body_yaw) * Quat::from_rotation_z(fall);
    // v0.16 Track B: `AvatarRenderer.setupRotations` while gliding: the body
    // tips toward the look (scaled in over the first ten ticks) and rolls
    // toward the turn between motion and look.
    if player.fall_flying {
        let ticks = player.fall_fly_ticks as f32 + appearance.alpha;
        let scale = (ticks * ticks / 100.0).clamp(0.0, 1.0);
        body *= Quat::from_rotation_x(scale * (-FRAC_PI_2 + player.pitch));
        let look = player.look_angle();
        let motion = player.vel;
        let (m, l) = (motion.with_y(0.0), look.with_y(0.0));
        if m.length_squared() > 1.0e-5 * 400.0 && l.length_squared() > 1.0e-5 {
            let dot = m.normalize().dot(l.normalize());
            let sign = motion.x * look.z - motion.z * look.x;
            let roll = (sign.signum() * dot.abs().min(1.0).acos()) as f32;
            body *= Quat::from_rotation_y(-roll);
        }
    }
    let light = (
        [(lighting.0.clamp(0.0, 1.0) * 255.0) as u8, 0, if appearance.hurt { 255 } else { 0 }, 0],
        (lighting.1.clamp(0.0, 1.0) * 255.0) as u8,
    );
    // v1.0 gap Track 8d: `EntityRenderDispatcher.submit`'s flame over the
    // player's box for its pose (standing, crouching, swimming), in block
    // light 15 and the player's sky light, invisible or not.
    if appearance.burning {
        let shape = player.collision_shape();
        let size = (shape.half_width as f32 * 2.0, shape.height as f32);
        let forward = if appearance.view == Vec3::ZERO { origin } else { appearance.view };
        super::model::flame(out, origin, size, forward, light.0[0], time);
    }
    let parts = pose(player, &appearance, time);
    const OVERLAYS: [[f32; 2]; 6] = [[32., 0.], [16., 32.], [40., 32.], [48., 48.], [0., 32.], [0., 48.]];
    for (i, p) in parts.iter().enumerate().filter(|_| !appearance.invisible) {
        textured_box(out, p, 0.0, origin, body, light, 1, 0, PLAYER_SCALE, UNTINTED);
        let mut overlay = *p;
        overlay.uv = OVERLAYS[i];
        textured_box(out, &overlay, if i == 0 { 0.5 } else { 0.25 }, origin, body, light, 1, 0, PLAYER_SCALE, UNTINTED);
    }
    draw_armor(out, &parts, &appearance.armor, origin, body, PLAYER_SCALE, light);
    if appearance.elytra {
        draw_wings(out, player, appearance.alpha, origin, body, light);
    }
    if let Some(item) = appearance.held {
        // v1.0 gap track 6c: the item's own model (a block's boxes, a 3D
        // item mesh or the extruded sprite) with its third-person display
        // transform, drawn by the item model pass.
        let arm = parts[2];
        let stack = appearance.held_stack.filter(|s| s.item == item).unwrap_or(crate::inventory::Stack::new(item, 1));
        let hand = third_person_hand(arm.pivot, arm.rot, origin, body, PLAYER_SCALE, false);
        let mut model =
            held_item(stack, hand, lighting, appearance.held_statue, appearance.held_icon, appearance.held_tint);
        // v1.0 gap track 6e: Java's trident item definition switches to
        // `item/trident_throwing` while it is used (`minecraft:using_item`).
        if appearance.aim == Aim::Trident && item == crate::weapons::TRIDENT {
            model.transform = hand
                * crate::content::item_display::Context::ThirdPersonRightHand
                    .matrix(&crate::content::item_display::TRIDENT_THROWING);
        }
        out.push_held(model);
    }
    if let Some(stack) = appearance.off_stack {
        // v1.0 final track F1: the left arm's grip, Java's offset of
        // (-1, 2, -10) sixteenths, and the left hand's display transform:
        // the right one's mirror (`ItemTransform.apply` with `leftHand`).
        let arm = parts[3];
        let hand = third_person_hand(arm.pivot, arm.rot, origin, body, PLAYER_SCALE, false)
            * Mat4::from_translation(Vec3::new(-2.0, 0.0, 0.0) / 16.0);
        let display = crate::content::item_display::held(&stack);
        let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let transform = hand
            * mirror
            * display.transform(crate::content::item_display::Context::ThirdPersonRightHand).matrix()
            * mirror
            * Mat4::from_translation(Vec3::splat(-0.5));
        let block = crate::world::copper_golem_statue::item_model(stack);
        let tint = crate::inventory::components::dye_tint(&stack);
        out.push_held(super::model::HeldItemModel {
            stack,
            transform,
            sky_light: lighting.0,
            block_light: lighting.1,
            block,
            icon: None,
            tint,
        });
    }
}

/// v0.16 Track B: `WingsLayer` with `ElytraModel`: two 10x20x2 wings
/// (inflated by one) hinged at the shoulders behind the body, turned by
/// `ElytraAnimationState`, mirrored across the body, in the original wing
/// texture (`content::elytra::WING_TEX`).
fn draw_wings(
    out: &mut Vec<EntityVertex>,
    player: &Player,
    alpha: f32,
    origin: Vec3,
    body: Quat,
    light: ([u8; 4], u8),
) {
    let [x, y, z] = player.elytra_rotation(alpha);
    let top = if player.crouching { 21.0 } else { 24.0 };
    // Java's model y points down and its back is +Z; this model's y is up
    // and its back -Z, which turns rotations about y and z around.
    let left = limb(
        [5.0, top, 0.0],
        [-10.0, -20.0, -4.0],
        [0.0, 0.0, -2.0],
        [0.0, 0.0],
        Quat::from_rotation_z(-z) * Quat::from_rotation_y(-y) * Quat::from_rotation_x(x),
    );
    let right = limb(
        [-5.0, top, 0.0],
        [0.0, -20.0, -4.0],
        [10.0, 0.0, -2.0],
        [0.0, 0.0],
        Quat::from_rotation_z(z) * Quat::from_rotation_y(y) * Quat::from_rotation_x(x),
    );
    for wing in [left, right] {
        textured_box(out, &wing, 1.0, origin, body, light, 2, crate::content::elytra::WING_TEX, PLAYER_SCALE, UNTINTED);
    }
}

/// HumanoidArmorLayer part order: head, body, right arm, left arm, right leg, left leg.
/// Outer pieces inflate by 1; leggings by 0.5. Leather draws an undyed trim just outside.
pub(crate) fn draw_armor(
    out: &mut Vec<EntityVertex>,
    parts: &[Limb; 6],
    worn: &[Option<super::armor::Worn>; 4],
    origin: Vec3,
    body: Quat,
    scale: f32,
    light: ([u8; 4], u8),
) {
    for (slot, piece) in worn.iter().enumerate() {
        let Some(piece) = *piece else { continue };
        let (indices, inflation, inner): (&[usize], f32, bool) = match slot {
            0 => (&[0], 1.0, false),
            1 => (&[1, 2, 3], 1.0, false),
            2 => (&[1, 4, 5], 0.5, true),
            _ => (&[4, 5], 1.0, false),
        };
        let paint = |out: &mut Vec<EntityVertex>, extra: f32, layer: u16, tint: [u8; 4]| {
            for &i in indices {
                textured_box(out, &parts[i], inflation + extra, origin, body, light, 3, layer, scale, tint);
            }
        };
        paint(out, 0.0, super::armor::layer(piece.kind, inner), super::armor::tint(&piece, false));
        if piece.kind == super::armor::ArmorKind::Leather {
            paint(out, 0.08, super::armor::leather_overlay(inner), super::armor::tint(&piece, true));
        }
        // v0.15 Track D: the armor trim, just outside the armor.
        if let Some((pattern, colour)) = piece.trim {
            paint(out, 0.04, super::armor::trim_layer(pattern, inner), super::armor::trim_tint(colour));
        }
    }
}

/// Original default skin, authored procedurally in Java's 64x64 layout.
/// All overlays are cutouts, so bare hands/face never become opaque shells.
pub fn skin_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; 64 * 64 * 4];
    let mut paint = |uv: [usize; 2], size: [usize; 3], kind: u8, overlay: bool| {
        let [w, h, d] = size;
        let faces = [
            [uv[0] + d, uv[1], w, d],
            [uv[0] + d + w, uv[1], w, d],
            [uv[0], uv[1] + d, d, h],
            [uv[0] + d, uv[1] + d, w, h],
            [uv[0] + d + w, uv[1] + d, d, h],
            [uv[0] + 2 * d + w, uv[1] + d, w, h],
        ];
        for (face, [u, v, width, height]) in faces.into_iter().enumerate() {
            for y in 0..height {
                for x in 0..width {
                    let mut color = match kind {
                        0 => {
                            if face == 0 || face == 5 || y < 2 {
                                [66, 43, 31]
                            } else {
                                [184, 129, 91]
                            }
                        }
                        1 => [34, 143, 159],
                        2 => {
                            if y < 4 {
                                [34, 143, 159]
                            } else {
                                [184, 129, 91]
                            }
                        }
                        _ => {
                            if y >= height - 2 {
                                [64, 57, 48]
                            } else {
                                [48, 58, 112]
                            }
                        }
                    };
                    if kind == 0 && face == 3 {
                        if y == 4 && (x == 1 || x == 6) {
                            color = [230, 228, 218];
                        }
                        if y == 4 && (x == 2 || x == 5) {
                            color = [57, 75, 111];
                        }
                        if y == 6 && (2..6).contains(&x) {
                            color = [110, 64, 44];
                        }
                    }
                    let visible = !overlay
                        || match kind {
                            0 => face == 0 || face == 5 || y == 0,
                            1 => y == height - 1 || (face == 3 && (x == 0 || x == width - 1)),
                            2 => y == 3,
                            _ => y == height - 3,
                        };
                    if !visible {
                        continue;
                    }
                    let n = ((x * 13 + y * 7 + face * 17) % 7) as i16 - 3;
                    let i = ((v + y) * 64 + u + x) * 4;
                    for c in 0..3 {
                        pixels[i + c] = (color[c] as i16 + n).clamp(0, 255) as u8;
                    }
                    pixels[i + 3] = 255;
                }
            }
        }
    };
    for (uv, size, kind) in [
        ([0, 0], [8, 8, 8], 0),
        ([16, 16], [8, 12, 4], 1),
        ([40, 16], [4, 12, 4], 2),
        ([32, 48], [4, 12, 4], 2),
        ([0, 16], [4, 12, 4], 3),
        ([16, 48], [4, 12, 4], 3),
    ] {
        paint(uv, size, kind, false);
    }
    for (uv, size, kind) in [
        ([32, 0], [8, 8, 8], 0),
        ([16, 32], [8, 12, 4], 1),
        ([40, 32], [4, 12, 4], 2),
        ([48, 48], [4, 12, 4], 2),
        ([0, 32], [4, 12, 4], 3),
        ([0, 48], [4, 12, 4], 3),
    ] {
        paint(uv, size, kind, true);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walk_uses_distance_and_java_tick_smoothing() {
        let mut w = WalkAnimation::default();
        w.update(0.2, 0.05, 0., false);
        assert!((w.speed - 0.32).abs() < 1e-6);
        assert!((w.position - 0.32).abs() < 1e-6);
        assert!((w.walk_dist - 0.12).abs() < 1e-6);
        w.update(0., 0.05, 0., false);
        assert!((w.speed - 0.192).abs() < 1e-6);
    }
    /// `ClientAvatarState.updateBob` eases 0.4 of the way each tick, and
    /// `resetBob` drops it at once, keeping the old value to interpolate from.
    #[test]
    fn bob_eases_toward_its_target_and_resets_when_riding() {
        let mut w = WalkAnimation::default();
        w.update_bob(0.1);
        assert!((w.bob - 0.04).abs() < 1e-6);
        w.update_bob(0.1);
        assert!((w.bob - 0.064).abs() < 1e-6 && (w.previous_bob - 0.04).abs() < 1e-6);
        w.reset_bob();
        assert_eq!((w.previous_bob, w.bob), (0.064, 0.0));
    }
    /// Java's bow and crossbow arm poses raise both arms forward and bring the
    /// hands toward the centre line (the screenshot of the held crossbow agrees).
    #[test]
    fn ranged_weapon_poses_raise_both_arms_toward_the_centre() {
        let p = Player::new(DVec3::ZERO);
        let toward_front = |aim: Aim| {
            let parts = pose(&p, &PlayerAppearance { alpha: 1., aim, ..Default::default() }, 0.);
            (parts[2].rot * Vec3::NEG_Y, parts[3].rot * Vec3::NEG_Y)
        };
        let (idle_right, idle_left) = toward_front(Aim::None);
        assert!(idle_right.z.abs() < 0.2 && idle_left.z.abs() < 0.2, "arms hang at rest");
        for (aim, forward) in [(Aim::Bow, 0.8), (Aim::CrossbowHold, 0.7), (Aim::CrossbowCharge(1.0), 0.45)] {
            let (right, left) = toward_front(aim);
            assert!(right.z > forward && left.z > forward, "{aim:?}: {right} {left}");
            // The right arm hangs at -x, so pointing at +x is toward the centre.
            assert!(right.x > 0.0 && left.x < 0.0, "{aim:?}: {right} {left}");
        }
        // Loading raises the off hand from 0.97 rad to level and across the body as it completes.
        let low = toward_front(Aim::CrossbowCharge(0.0)).1;
        let done = toward_front(Aim::CrossbowCharge(1.0)).1;
        assert!(low.y < -0.3 && done.y.abs() < 0.05 && done.x < low.x, "{low} {done}");
    }
    #[test]
    fn opposing_limbs_and_sneak_pivots_match_java() {
        let mut p = Player::new(DVec3::ZERO);
        p.animation.speed = 1.;
        p.animation.position = 0.;
        let a = PlayerAppearance { alpha: 1., ..Default::default() };
        let parts = pose(&p, &a, 0.);
        assert!((parts[4].rot * Vec3::NEG_Y).z < -0.9);
        assert!((parts[5].rot * Vec3::NEG_Y).z > 0.9);
        assert_eq!(parts[0].pivot.y, 24.);
        p.pose = PlayerPose::Crouching;
        let parts = pose(&p, &a, 0.);
        assert_eq!(parts[0].pivot.y, 19.8);
        assert_eq!(parts[4].pivot.z, -4.);
    }
    #[test]
    fn outer_armor_sits_one_pixel_outside_leggings() {
        let body = limb([0.; 3], [-4., -12., -2.], [4., 0., 2.], [16., 16.], Quat::IDENTITY);
        let parts = [body; 6];
        let mut worn = [None; 4];
        worn[1] = Some(crate::entity::armor::Worn {
            kind: crate::entity::armor::ArmorKind::Iron,
            glint: false,
            dye: None,
            trim: None,
        });
        worn[2] = Some(crate::entity::armor::Worn {
            kind: crate::entity::armor::ArmorKind::Iron,
            glint: true,
            dye: None,
            trim: None,
        });
        let mut out = Vec::new();
        draw_armor(&mut out, &parts, &worn, Vec3::ZERO, Quat::IDENTITY, 1.0, ([255, 0, 0, 0], 0));
        let extent = |inner: bool| {
            out.iter()
                .filter(|v| v.torch[1] == 3 && (v.torch[2].is_multiple_of(2) == inner))
                .map(|v| v.pos[0])
                .fold(0.0f32, f32::max)
        };
        assert!((extent(false) - 5.0 / 16.0).abs() < 1e-5);
        assert!((extent(true) - 4.5 / 16.0).abs() < 1e-5);
        assert!(out.iter().any(|v| v.color[3] == 255));
    }

    #[test]
    fn skin_has_a_face_and_transparent_overlays() {
        let skin = skin_pixels();
        assert_eq!(skin.len(), 64 * 64 * 4);
        assert_eq!(skin[(12 * 64 + 10) * 4 + 3], 255);
        assert_eq!(skin[(12 * 64 + 42) * 4 + 3], 0);
    }
}
