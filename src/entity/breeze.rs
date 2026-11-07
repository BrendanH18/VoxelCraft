//! The breeze (Java 26.3 `Breeze`, `BreezeAi`, `Shoot`, `LongJump`,
//! `Slide`, `ShootWhenStuck`): its brain memories as tick counters. The
//! behaviours run in `mob::breeze`; wind charges in `wind`.
use glam::DVec3;
use serde_json::{Value, json};

/// `Breeze.createAttributes`: movement speed 0.63, follow range 24.
pub const MOVEMENT_SPEED: f32 = 0.63;
pub const FOLLOW_RANGE: f64 = 24.0;
/// `BreezeAi.SPEED_MULTIPLIER_WHEN_SLIDING`.
pub const SLIDE_SPEED: f32 = 0.6;
/// `BreezeAi.JUMP_CIRCLE_*_RADIUS`.
pub const INNER_RADIUS: f64 = 4.0;
pub const MIDDLE_RADIUS: f64 = 8.0;
/// `Shoot` timings (ticks) and range (`ATTACK_RANGE_MAX_SQRT` = 256, 16 blocks).
pub const SHOOT_CHARGE: u32 = 15;
pub const SHOOT_RECOVER: u32 = 4;
pub const SHOOT_COOLDOWN: u32 = 10;
pub const SHOOT_RANGE_SQR: f64 = 256.0;
/// `LongJump` timings and limits.
pub const INHALE: u32 = 10;
pub const JUMP_COOLDOWN: u32 = 10;
pub const JUMP_COOLDOWN_HURT: u32 = 2;
pub const JUMP_ANGLES: [u32; 5] = [40, 55, 60, 75, 80];
/// `MAX_JUMP_VELOCITY_MULTIPLIER` times the follow range: 1.4 blocks a tick.
pub const MAX_JUMP_SPEED: f32 = 0.058_333_334 * 24.0;
/// `BREEZE_SHOOT` lifetimes after landing and after a slide or when stuck.
pub const SHOOT_AFTER_JUMP: u32 = 100;
pub const SHOOT_AFTER_SLIDE: u32 = 60;
/// `SlideToTargetSink(20, 40)`: a slide lasts this many ticks at most.
pub const SLIDE_MIN: u32 = 20;
pub const SLIDE_MAX: u32 = 40;
/// `Breeze.getFiringYPosition`: half the 1.77 height plus 0.3.
pub const HEIGHT: f64 = 1.77;

/// `Pose`s the breeze uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pose {
    #[default]
    Standing,
    Inhaling,
    LongJumping,
    Shooting,
}

impl Pose {
    pub fn index(self) -> u8 {
        self as u8
    }
    pub fn from_index(i: u64) -> Pose {
        match i {
            1 => Pose::Inhaling,
            2 => Pose::LongJumping,
            3 => Pose::Shooting,
            _ => Pose::Standing,
        }
    }
}

/// Brain memories, counted down in ticks (0 is absent).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub pose: Pose,
    /// `BREEZE_SHOOT`.
    pub shoot: u32,
    pub shoot_charging: u32,
    pub shoot_recovering: u32,
    pub shoot_cooldown: u32,
    /// `BREEZE_JUMP_INHALING`, `BREEZE_JUMP_COOLDOWN`, `BREEZE_JUMP_TARGET`.
    pub inhaling: u32,
    pub jump_cooldown: u32,
    pub jump_target: Option<DVec3>,
    /// The jump has left the ground (landing ends it).
    pub airborne: bool,
    /// `BREEZE_LEAVING_WATER`.
    pub leaving_water: bool,
    /// Slide walk target and ticks left.
    pub walk_target: Option<DVec3>,
    pub walk_left: u32,
    /// Ticks since last hurt (`HURT_BY` memory), for the shorter cooldown.
    pub since_hurt: u32,
    /// Fractional seconds not yet spent on whole brain ticks.
    pub clock: f32,
    /// Ticks until the next whirl sound (1–80).
    pub whirl: u32,
}

impl State {
    pub fn save(&self) -> Value {
        json!({
            "pose": self.pose.index(),
            "shoot": self.shoot,
            "shoot_charging": self.shoot_charging,
            "shoot_recovering": self.shoot_recovering,
            "shoot_cooldown": self.shoot_cooldown,
            "inhaling": self.inhaling,
            "jump_cooldown": self.jump_cooldown,
            "jump_target": self.jump_target.map(|t| t.to_array()),
            "airborne": self.airborne,
            "leaving_water": self.leaving_water,
        })
    }

    pub fn load(v: &Value) -> Self {
        let n = |k: &str| v[k].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0).min(1000);
        let target = v["jump_target"].as_array().and_then(|a| {
            let f = |i: usize| a.get(i).and_then(Value::as_f64);
            Some(DVec3::new(f(0)?, f(1)?, f(2)?)).filter(|p| p.is_finite())
        });
        Self {
            pose: Pose::from_index(v["pose"].as_u64().unwrap_or(0)),
            shoot: n("shoot"),
            shoot_charging: n("shoot_charging"),
            shoot_recovering: n("shoot_recovering"),
            shoot_cooldown: n("shoot_cooldown"),
            inhaling: n("inhaling"),
            jump_cooldown: n("jump_cooldown"),
            jump_target: target,
            airborne: v["airborne"] == true,
            leaving_water: v["leaving_water"] == true,
            since_hurt: 1000,
            ..Default::default()
        }
    }
}

/// Track C sound cues: the breeze, wind charges, trial spawners, vaults
/// and ominous bottles (original synthesized sounds, Java event names in
/// the comments).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// `BREEZE_WHIRL` / idle air.
    Whirl,
    /// `BREEZE_SHOOT`.
    Shoot,
    /// `BREEZE_INHALE` (charging a shot).
    Inhale,
    /// `BREEZE_CHARGE` (inhaling for a jump).
    Charge,
    /// `BREEZE_JUMP`.
    Jump,
    /// `BREEZE_LAND`.
    Land,
    /// `BREEZE_DEFLECT`.
    Deflect,
    /// `WIND_CHARGE_BURST` (also breeze wind charges).
    Burst,
    /// `WIND_CHARGE_THROW`.
    Throw,
    /// `TRIAL_SPAWNER_DETECT_PLAYER` (also the ominous variant).
    SpawnerDetect,
    /// `TRIAL_SPAWNER_SPAWN_MOB`.
    SpawnerSpawn,
    /// `TRIAL_SPAWNER_OPEN_SHUTTER`.
    SpawnerOpen,
    /// `TRIAL_SPAWNER_EJECT_ITEM`.
    SpawnerEject,
    /// `TRIAL_SPAWNER_CLOSE_SHUTTER`.
    SpawnerClose,
    /// `TRIAL_SPAWNER_OMINOUS_ACTIVATE`.
    SpawnerOminous,
    /// `TRIAL_SPAWNER_SPAWN_ITEM_BEGIN` (an ominous item spawner appears).
    ItemSpawnerBegin,
    /// `TRIAL_SPAWNER_SPAWN_ITEM` (it lets go).
    ItemSpawnerDrop,
    /// `VAULT_ACTIVATE` / `VAULT_DEACTIVATE`.
    VaultActivate,
    VaultDeactivate,
    /// `VAULT_INSERT_ITEM`, `VAULT_INSERT_ITEM_FAIL`, `VAULT_REJECT_REWARDED_PLAYER`.
    VaultInsert,
    VaultFail,
    VaultReject,
    /// `VAULT_OPEN_SHUTTER`, `VAULT_EJECT_ITEM`, `VAULT_CLOSE_SHUTTER`.
    VaultOpen,
    VaultEject,
    VaultClose,
    /// `OMINOUS_BOTTLE_DISPOSE` and `APPLY_EFFECT_TRIAL_OMEN`.
    OminousBottle,
    TrialOmen,
}

impl Cue {
    pub const ALL: [Cue; 27] = [
        Cue::Whirl,
        Cue::Shoot,
        Cue::Inhale,
        Cue::Charge,
        Cue::Jump,
        Cue::Land,
        Cue::Deflect,
        Cue::Burst,
        Cue::Throw,
        Cue::SpawnerDetect,
        Cue::SpawnerSpawn,
        Cue::SpawnerOpen,
        Cue::SpawnerEject,
        Cue::SpawnerClose,
        Cue::SpawnerOminous,
        Cue::ItemSpawnerBegin,
        Cue::ItemSpawnerDrop,
        Cue::VaultActivate,
        Cue::VaultDeactivate,
        Cue::VaultInsert,
        Cue::VaultFail,
        Cue::VaultReject,
        Cue::VaultOpen,
        Cue::VaultEject,
        Cue::VaultClose,
        Cue::OminousBottle,
        Cue::TrialOmen,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Mob, MobKind};

    #[test]
    fn the_breeze_keeps_its_reserved_wire_index_and_saves() {
        assert_eq!(MobKind::Breeze.lan_index(), 73);
        assert_eq!(MobKind::from_lan_index(73), Some(MobKind::Breeze));
        assert_eq!(MobKind::from_lan_index(72), None, "Track A's warden slot");
        assert_eq!(MobKind::from_lan_index(71), Some(MobKind::Sniffer));
        assert_eq!(MobKind::from_name("breeze"), Some(MobKind::Breeze));
        assert_eq!(MobKind::Breeze.max_health(), 30.0);
        assert!(
            MobKind::Breeze.is_hostile() && !MobKind::Breeze.spawns_in(crate::world::terrain::Dimension::Overworld)
        );
        let mut m = Mob::new(MobKind::Breeze, DVec3::new(1.5, 70.0, -2.5), 0.3);
        {
            let s = m.breeze.as_mut().unwrap();
            s.pose = Pose::Inhaling;
            s.inhaling = 4;
            s.jump_cooldown = 7;
            s.shoot = 55;
            s.jump_target = Some(DVec3::new(3.5, 70.0, 1.5));
        }
        m.keeps_equipment = true;
        let saved = crate::entity::nether::save_mob(&m);
        let back = crate::entity::nether::load_mob(&saved).unwrap();
        let (a, b) = (m.breeze.as_ref().unwrap(), back.breeze.as_ref().unwrap());
        assert_eq!((b.pose, b.inhaling, b.jump_cooldown, b.shoot, b.jump_target), (a.pose, 4, 7, 55, a.jump_target));
        assert!(back.keeps_equipment);
    }

    #[test]
    fn jump_and_shot_constants_follow_java() {
        assert!((MAX_JUMP_SPEED - 1.4).abs() < 1e-4);
        assert_eq!((SHOOT_CHARGE, SHOOT_RECOVER, SHOOT_COOLDOWN, INHALE), (15, 4, 10, 10));
        assert_eq!(JUMP_ANGLES, [40, 55, 60, 75, 80]);
    }
}
