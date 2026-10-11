//! Compact render state for replicated mobs (LAN clients draw what the host simulates).
//! Only fields models read are sent; gameplay state stays on the host.
use super::nether::Weapon;
use super::{Mob, armor::ArmorKind, villager::Profession};
use crate::{color::DyeColor, inventory::StackName, item::Item};
use glam::DVec3;
use serde_json::{Value, json};
use std::f32::consts::TAU;

fn dye(v: &Value) -> Option<DyeColor> {
    DyeColor::ALL.get(usize::try_from(v.as_u64()?).ok()?).copied()
}
fn small(v: &Value) -> Option<u8> {
    v.as_u64().and_then(|n| u8::try_from(n).ok())
}

impl Mob {
    /// What a remote client needs to draw this mob, beyond its position and pose.
    pub fn look(&self) -> Value {
        let armor: Vec<i64> = self
            .armor
            .iter()
            .map(|a| a.and_then(|k| ArmorKind::ALL.iter().position(|x| *x == k)).map_or(-1, |i| i as i64))
            .collect();
        let mut v = json!({
            "wool": DyeColor::ALL.iter().position(|c| *c == self.wool_color).unwrap_or(0),
            "sheared": self.sheared,
            "armor": armor,
            "glint": self.armor_glint,
            "charged": self.charged,
            "fuse": self.fuse,
            "burning": self.burning,
            "attack": self.attack_anim,
        });
        if let Some(name) = self.name.as_str() {
            v["name"] = json!(name);
        }
        if let Some(p) = self.leash_pos {
            v["leash"] = json!(p.to_array());
            v["knot"] = json!(matches!(self.leash, Some(super::leash::Leash::Fence(_))));
        }
        if let Some(a) = &self.animal {
            v["animal"] = json!({
                "variant": a.variant,
                "collar": DyeColor::ALL.iter().position(|c| *c == a.collar).unwrap_or(14),
                "sitting": a.sitting,
                "sleeping": a.sleeping,
                "begging": a.begging,
                "tame": a.owner.is_some(),
                "horns": a.horns,
            });
        }
        if let Some(s) = &self.mount {
            v["mount"] = json!({
                "variant": s.variant,
                "marking": s.marking,
                "chest": s.chest,
                "saddle": s.saddled(),
                "armor": s.slots[1].map_or(0, |a| a.item.0),
            });
        }
        if let Some(a) = &self.aquatic {
            v["aquatic"] = json!({"variant": a.variant, "puff": a.puff});
        }
        if let Some(villager) = &self.villager {
            v["villager"] = json!({
                "profession": Profession::ALL.iter().position(|p| *p == villager.profession).unwrap_or(0),
                "sleeping": villager.sleeping,
            });
        }
        if let Some(n) = &self.nether {
            v["weapon"] = json!(n.weapon.name());
        }
        v
    }

    /// Applies [`Mob::look`] from the host; missing fields keep their previous values.
    pub fn apply_look(&mut self, v: &Value) {
        if let Some(c) = dye(&v["wool"]) {
            self.wool_color = c;
        }
        self.sheared = v["sheared"] == true;
        if let Some(armor) = v["armor"].as_array() {
            for (slot, a) in self.armor.iter_mut().zip(armor) {
                *slot = a.as_u64().and_then(|i| ArmorKind::ALL.get(i as usize).copied());
            }
        }
        self.armor_glint = small(&v["glint"]).unwrap_or(0);
        self.charged = v["charged"] == true;
        self.fuse = v["fuse"].as_f64().unwrap_or(0.0) as f32;
        self.burning = v["burning"] == true;
        self.attack_anim = v["attack"].as_f64().unwrap_or(0.0) as f32;
        self.name = v["name"].as_str().and_then(StackName::new).unwrap_or_default();
        self.leash_pos = v["leash"].as_array().and_then(|a| {
            let p = DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?);
            p.is_finite().then_some(p)
        });
        self.leash = self.leash_pos.map(|p| {
            if v["knot"] == true {
                super::leash::Leash::Fence(p.floor().as_ivec3())
            } else {
                super::leash::Leash::Player(super::PlayerId::HOST)
            }
        });
        if let (Some(a), w) = (self.animal.as_mut(), &v["animal"])
            && w.is_object()
        {
            a.variant = small(&w["variant"]).unwrap_or(a.variant);
            a.collar = dye(&w["collar"]).unwrap_or(a.collar);
            a.sitting = w["sitting"] == true;
            a.sleeping = w["sleeping"] == true;
            a.begging = w["begging"] == true;
            a.horns = small(&w["horns"]).unwrap_or(a.horns).min(2);
            // Only "is tamed" matters for drawing (collars).
            a.owner = (w["tame"] == true).then_some(super::PlayerId::HOST);
        }
        if let (Some(s), w) = (self.mount.as_mut(), &v["mount"])
            && w.is_object()
        {
            s.variant = small(&w["variant"]).unwrap_or(0).min(6);
            s.marking = small(&w["marking"]).unwrap_or(0).min(4);
            s.chest = w["chest"] == true;
            s.slots[0] = (w["saddle"] == true).then(|| crate::inventory::Stack::new(Item::SADDLE, 1));
            s.slots[1] = w["armor"]
                .as_u64()
                .and_then(|i| u16::try_from(i).ok())
                .map(Item)
                .filter(|i| super::mounts::armor_points(*i) > 0)
                .map(|i| crate::inventory::Stack::new(i, 1));
        }
        if let (Some(a), w) = (self.aquatic.as_mut(), &v["aquatic"])
            && w.is_object()
        {
            a.variant = small(&w["variant"]).unwrap_or(a.variant);
            a.puff = small(&w["puff"]).unwrap_or(0).min(2);
        }
        if let (Some(villager), w) = (self.villager.as_mut(), &v["villager"])
            && w.is_object()
        {
            villager.profession = w["profession"]
                .as_u64()
                .and_then(|i| Profession::ALL.get(i as usize).copied())
                .unwrap_or(villager.profession);
            villager.sleeping = w["sleeping"] == true;
        }
        if let (Some(n), Some(w)) = (self.nether.as_mut(), v["weapon"].as_str()) {
            n.weapon = Weapon::from_name(w);
        }
    }

    /// Advances the walk cycle of a replicated mob from its observed movement, as
    /// the host's physics does from velocity.
    pub fn animate_replica(&mut self, dt: f32) {
        let moved = (self.pos - self.previous_pos) * DVec3::new(1.0, 0.0, 1.0);
        let hspeed = (moved.length() / dt as f64) as f32;
        self.limb_phase = (self.limb_phase + hspeed * dt * 3.2) % (TAU * 64.0);
        let amp = (hspeed / 2.0).min(1.0);
        self.limb_amp += (amp - self.limb_amp) * (dt * 10.0).min(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MobKind;

    #[test]
    fn look_round_trips_render_state() {
        let mut horse = Mob::new(MobKind::Horse, DVec3::ZERO, 0.0);
        let s = horse.mount.as_mut().unwrap();
        s.variant = 4;
        s.marking = 2;
        s.slots[0] = Some(crate::inventory::Stack::new(Item::SADDLE, 1));
        s.slots[1] = Some(crate::inventory::Stack::new(Item::DIAMOND_HORSE_ARMOR, 1));
        horse.name = StackName::new("Spirit").unwrap();
        horse.leash_pos = Some(DVec3::new(1.5, 2.7, 3.5));
        horse.leash = Some(super::super::leash::Leash::Fence(glam::IVec3::new(1, 2, 3)));
        let mut copy = Mob::new(MobKind::Horse, DVec3::ZERO, 0.0);
        copy.apply_look(&horse.look());
        let c = copy.mount.as_ref().unwrap();
        assert_eq!((c.variant, c.marking), (4, 2));
        assert!(c.saddled());
        assert_eq!(c.slots[1].unwrap().item, Item::DIAMOND_HORSE_ARMOR);
        assert_eq!(copy.name.as_str(), Some("Spirit"));
        assert_eq!(copy.leash_pos, horse.leash_pos);
        assert!(matches!(copy.leash, Some(super::super::leash::Leash::Fence(_))));

        let mut wolf = Mob::new(MobKind::Wolf, DVec3::ZERO, 0.0);
        let a = wolf.animal.as_mut().unwrap();
        a.owner = Some(super::super::PlayerId(9));
        a.sitting = true;
        a.collar = DyeColor::Blue;
        let mut copy = Mob::new(MobKind::Wolf, DVec3::ZERO, 0.0);
        copy.apply_look(&wolf.look());
        let a = copy.animal.as_ref().unwrap();
        assert!(a.sitting && a.owner.is_some());
        assert_eq!(a.collar, DyeColor::Blue);

        let mut sheep = Mob::new(MobKind::Sheep, DVec3::ZERO, 0.0);
        sheep.wool_color = DyeColor::Pink;
        sheep.sheared = true;
        let mut copy = Mob::new(MobKind::Sheep, DVec3::ZERO, 0.0);
        copy.apply_look(&sheep.look());
        assert_eq!(copy.wool_color, DyeColor::Pink);
        assert!(copy.sheared);
    }
}
