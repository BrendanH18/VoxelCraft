//! Java 26.3 elytra (v0.16 Track B, item 1675): a chest-slot glider
//! (`DataComponents.GLIDER` with `Equippable(CHEST)`), durability 432,
//! epic rarity, repaired with phantom membrane, taking Unbreaking, Mending
//! and the curses. Gliding itself (`LivingEntity.travelFallFlying`) runs
//! in `player::travel`; this module holds the item rules and Java's
//! per-tick glide maths so every path shares them. Numbers are checked
//! against `tests/fixtures/java/26.3/flight.json`.

use crate::inventory::Stack;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};
use glam::DVec3;

/// `Items.ELYTRA`.
pub const ELYTRA: Item = Item(1675);
/// `Properties.durability(432)`.
pub const DURABILITY: u16 = 432;
/// `LivingEntity.TICKS_PER_ELYTRA_FREE_FALL_EVENT` and
/// `FREE_FALL_EVENTS_PER_ELYTRA_BREAK`: one use every twenty ticks of flight.
pub const TICKS_PER_FREE_FALL_EVENT: u32 = 10;
pub const FREE_FALL_EVENTS_PER_BREAK: u32 = 2;
/// `ELYTRA_HORIZONTAL_AIR_DRAG` and `ELYTRA_VERTICAL_AIR_DRAG`.
pub const HORIZONTAL_DRAG: f32 = 0.99;
pub const VERTICAL_DRAG: f32 = 0.98;
/// `Pose.FALL_FLYING`'s player dimensions: 0.6 wide and tall, eyes at 0.4.
pub const POSE_SIZE: f64 = 0.6;
pub const POSE_EYE: f64 = 0.4;

/// The worn wings' texture layer (Track B's block texture layers 609–668;
/// 660, after the shulker boxes).
pub const WING_TEX: u16 = 660;
const _: () = assert!(WING_TEX >= crate::world::shulker_box::TEX_END && WING_TEX < 669);

/// Original wing texture: a grey-violet membrane with darker ribs fanning
/// from the hinge and a pale leading edge.
pub fn wing_pixel(x: usize, y: usize) -> [u8; 4] {
    let (x, y) = (x as i32, y as i32);
    let rib = (x * 3 - y).rem_euclid(7) == 0 || (x + y * 2).rem_euclid(9) == 0;
    let edge = x <= 1 || y <= 1;
    let grain = ((x * 13 + y * 7) % 5) * 3;
    let c: [i32; 3] = if edge {
        [196, 190, 214]
    } else if rib {
        [98, 90, 124]
    } else {
        [146 + grain, 140 + grain, 170 + grain]
    };
    [c[0] as u8, c[1] as u8, c[2] as u8, 255]
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    (item == ELYTRA).then_some(ItemInfo {
        name: "elytra",
        kind: ItemKind::Material,
        max_stack: 1,
        sprite: Sprite::Elytra,
    })
}

/// `LivingEntity.canGlideUsing` for the chest slot: an elytra whose next
/// use won't break it (`!nextDamageWillBreak`: at one durability left it
/// stops working but stays).
pub fn can_glide_with(stack: Option<Stack>) -> bool {
    stack.is_some_and(|s| s.item == ELYTRA && s.damage < DURABILITY - 1)
}

/// Whether an elytra is at its last durability point: Java draws
/// `elytra_broken` for it.
pub fn is_broken(stack: &Stack) -> bool {
    stack.item == ELYTRA && stack.damage >= DURABILITY - 1
}

/// `LivingEntity.updateFallFlying`: whether the glider is used this tick,
/// with `fall_fly_ticks` the ticks flown before it (Java's `fallFlyTicks`).
pub fn wears_this_tick(fall_fly_ticks: u32) -> bool {
    let check = fall_fly_ticks + 1;
    check.is_multiple_of(TICKS_PER_FREE_FALL_EVENT)
        && (check / TICKS_PER_FREE_FALL_EVENT).is_multiple_of(FREE_FALL_EVENTS_PER_BREAK)
}

/// Java's `Entity.calculateViewVector` for the yaw and pitch Java stores
/// (degrees): pitch down positive, yaw 0 south.
pub fn look_angle(java_yaw: f32, java_pitch: f32) -> DVec3 {
    use crate::knockback::{mth_cos, mth_sin};
    let x_rad = java_pitch * (std::f64::consts::PI / 180.0) as f32;
    let y_rad = -java_yaw * (std::f64::consts::PI / 180.0) as f32;
    let (y_cos, y_sin) = (mth_cos(f64::from(y_rad)), mth_sin(f64::from(y_rad)));
    let (x_cos, x_sin) = (mth_cos(f64::from(x_rad)), mth_sin(f64::from(x_rad)));
    DVec3::new(f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos))
}

/// `LivingEntity.updateFallFlyingMovement`: the glide's lift, pitch-dive
/// conversion and drag, in blocks per tick. `java_pitch` is Java's `xRot`
/// in degrees (looking down positive) and `look` its view vector.
pub fn glide(movement: DVec3, look: DVec3, java_pitch: f32, gravity: f64) -> DVec3 {
    let lean = java_pitch * (std::f64::consts::PI / 180.0) as f32;
    let look_hor = (look.x * look.x + look.z * look.z).sqrt();
    let move_hor = (movement.x * movement.x + movement.z * movement.z).sqrt();
    // `Mth.square(Math.cos(leanAngle))`: the exact cosine, unlike `Mth.sin` below.
    let lift = f64::from(lean).cos().powi(2);
    let mut m = movement + DVec3::new(0.0, gravity * (-1.0 + lift * 0.75), 0.0);
    if m.y < 0.0 && look_hor > 0.0 {
        let convert = m.y * -0.1 * lift;
        m += DVec3::new(look.x * convert / look_hor, convert, look.z * convert / look_hor);
    }
    if lean < 0.0 && look_hor > 0.0 {
        let convert = move_hor * f64::from(-crate::knockback::mth_sin(f64::from(lean))) * 0.04;
        m += DVec3::new(-look.x * convert / look_hor, convert * 3.2, -look.z * convert / look_hor);
    }
    if look_hor > 0.0 {
        m += DVec3::new((look.x / look_hor * move_hor - m.x) * 0.1, 0.0, (look.z / look_hor * move_hor - m.z) * 0.1);
    }
    m * DVec3::new(f64::from(HORIZONTAL_DRAG), f64::from(VERTICAL_DRAG), f64::from(HORIZONTAL_DRAG))
}

/// `LivingEntity.handleFallFlyingCollisions`: a wall that stopped the glide
/// hurts by ten times the horizontal speed lost, less three (`fly_into_wall`).
/// Speeds are blocks per tick; `None` below one damage.
pub fn wall_damage(speed_before: f64, speed_after: f64) -> Option<f32> {
    let damage = ((speed_before - speed_after) * 10.0 - 3.0) as f32;
    (damage > 0.0).then_some(damage)
}

/// `FireworkRocketEntity.tick` for a rocket held by a gliding entity: the
/// push toward 1.5 blocks a tick along the look.
pub fn rocket_boost(movement: DVec3, look: DVec3) -> DVec3 {
    movement
        + DVec3::new(
            look.x * 0.1 + (look.x * 1.5 - movement.x) * 0.5,
            look.y * 0.1 + (look.y * 1.5 - movement.y) * 0.5,
            look.z * 0.1 + (look.z * 1.5 - movement.z) * 0.5,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durability_stops_gliding_at_the_last_point() {
        let mut s = Stack::new(ELYTRA, 1);
        assert!(can_glide_with(Some(s)));
        s.damage = DURABILITY - 2;
        assert!(can_glide_with(Some(s)) && !is_broken(&s));
        s.damage = DURABILITY - 1;
        assert!(!can_glide_with(Some(s)) && is_broken(&s));
        assert!(!can_glide_with(Some(Stack::new(Item::DIAMOND, 1))));
        assert_eq!(ELYTRA.durability(), Some(DURABILITY));
        // A use every twenty ticks: the twentieth, fortieth, ... tick of flight.
        let worn: Vec<u32> = (0..60).filter(|&t| wears_this_tick(t)).collect();
        assert_eq!(worn, [19, 39, 59]);
    }

    #[test]
    fn level_flight_sinks_and_diving_speeds_up() {
        // Looking level: gravity mostly cancelled by lift (a quarter acts).
        let look = look_angle(0.0, 0.0);
        let m = glide(DVec3::new(0.0, 0.0, 1.0), look, 0.0, 0.08);
        assert!((m.y - (-0.02 * 0.98 - 0.0)).abs() < 0.01, "{m}");
        // Diving converts height into speed.
        let mut dive = DVec3::new(0.0, -0.5, 0.5);
        let look = look_angle(0.0, 45.0);
        for _ in 0..20 {
            dive = glide(dive, look, 45.0, 0.08);
        }
        assert!(dive.z > 0.5, "{dive}");
        assert_eq!(wall_damage(1.0, 0.0), Some(7.0));
        assert_eq!(wall_damage(0.3, 0.0), None);
    }
}
