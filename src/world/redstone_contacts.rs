//! Entity contact inputs. Work scales with entities touching block cells,
//! never the area/number of loaded pressure plates.
use super::{
    World,
    redstone_blocks::{self as r, Component},
};
use crate::entity::Entities;
use glam::{DVec3, IVec3};

#[derive(Clone, Copy, Default)]
pub(super) struct Contacts {
    pub living: u32,
    pub all: u32,
    pub arrows: bool,
    /// Minecarts touching a detector rail's search box.
    pub carts: u32,
}

impl World {
    /// Called before tick_redstone. Players include host, pads and CLI agents;
    /// Spectator/dead players must be omitted by the caller.
    pub fn redstone_contacts(
        &mut self,
        players: impl IntoIterator<Item = (DVec3, crate::physics::Shape)>,
        entities: &Entities,
    ) {
        self.redstone.contacts.clear();
        for (pos, shape) in players {
            self.redstone_contact_box(shape.aabb(pos), true, false, false);
        }
        for mob in &entities.mobs {
            if mob.alive() {
                self.redstone_contact_box(mob.aabb(), true, false, false);
            }
        }
        for item in &entities.items {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(item.pos), false, false, false);
        }
        for arrow in &entities.arrows {
            self.redstone_contact_box(crate::physics::Shape::new(0.25, 0.5).aabb(arrow.pos), false, true, false);
        }
        for orb in &entities.orbs {
            self.redstone_contact_box(orb.aabb(), false, false, false);
        }
        for tnt in &entities.tnt {
            self.redstone_contact_box(crate::physics::Shape::new(0.49, 0.98).aabb(tnt.pos), false, false, false);
        }
        for pearl in &entities.pearls {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(pearl.pos), false, false, false);
        }
        // Splash potions stand on their position like Java's 0.25-block entity box.
        for potion in &entities.potions {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(potion.pos), false, false, false);
        }
        // Fishing bobbers are 0.25-block entities too, so a wooden plate feels them.
        for bobber in &entities.bobbers {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(bobber.pos), false, false, false);
        }
        for thrown in &entities.thrown {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(thrown.pos), false, false, false);
        }
        for fireball in &entities.fireballs {
            let width = if fireball.is_large() { 1.0 } else { 0.3125 };
            self.redstone_contact_box(
                crate::physics::Shape::new(width / 2.0, width).aabb(fireball.pos),
                false,
                false,
                false,
            );
        }
        for eye in &entities.eyes {
            self.redstone_contact_box(crate::physics::Shape::new(0.125, 0.25).aabb(eye.pos), false, false, false);
        }
        for cart in &entities.minecarts {
            // Detector rails find only `AbstractMinecart`s; boats and
            // cushions share this list but press plates as other entities.
            let minecart = cart.kind.boat().is_none() && cart.kind.cushion().is_none();
            self.redstone_contact_box(cart.aabb(), false, false, minecart);
        }
        // Iterate by temporarily taking the retained map; no per-tick allocation.
        let contacts = std::mem::take(&mut self.redstone.contacts);
        for (&p, c) in &contacts {
            match self.get_block(p).and_then(r::component) {
                Some(Component::Plate { kind, power: 0 }) => {
                    let power = plate_power(kind, *c);
                    if power > 0 {
                        let b = self.get_block(p).unwrap_or_default();
                        self.edit(p, super::nether_biome_blocks::keep_wood(b, r::plate(kind, power)), false);
                        self.schedule_redstone(p, if kind < 2 { 20 } else { 10 }, 0);
                    }
                }
                Some(Component::Button { on: false, wood: true, .. }) if c.arrows => {
                    self.use_redstone(p);
                }
                _ => {}
            }
        }
        self.redstone.contacts = contacts;
        self.refresh_tripwires();
        self.power_detectors();
    }

    fn power_detectors(&mut self) {
        let occupied: Vec<IVec3> =
            self.redstone.contacts.iter().filter(|(_, c)| c.carts > 0).map(|(&p, _)| p).collect();
        for p in occupied {
            let Some(b) = self.get_block(p) else { continue };
            if super::rails::kind(b) != Some(super::rails::RailKind::Detector) {
                continue;
            }
            if !super::rails::is_powered(b) {
                self.edit(p, super::rails::with_power(b, true), false);
            }
            self.schedule_redstone(p, 20, 0);
        }
    }

    fn redstone_contact_box(&mut self, (min, max): (DVec3, DVec3), living: bool, arrow: bool, cart: bool) {
        if living {
            let foot = (min + DVec3::new((max.x - min.x) * 0.5, -0.01, (max.z - min.z) * 0.5)).floor().as_ivec3();
            self.touch_redstone_ore(foot);
        }
        let lo = min.floor().as_ivec3();
        let hi = max.floor().as_ivec3();
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let p = IVec3::new(x, y, z);
                    let Some(b) = self.get_block(p) else { continue };
                    let bounds = match r::component(b) {
                        Some(Component::Plate { .. }) => {
                            Some((DVec3::new(0.0625, 0.0, 0.0625), DVec3::new(0.9375, 0.25, 0.9375)))
                        }
                        Some(Component::Button { wood: true, .. }) if arrow => {
                            super::shape::shape(b, |_| super::block::Block::AIR, super::block::Block::AIR).bounds().map(
                                |b| {
                                    (
                                        DVec3::from_array(b.min.map(|v| v as f64 / 16.0)),
                                        DVec3::from_array(b.max.map(|v| v as f64 / 16.0)),
                                    )
                                },
                            )
                        }
                        _ if cart && super::rails::kind(b) == Some(super::rails::RailKind::Detector) => {
                            Some(super::rails::DETECTOR_BOX)
                        }
                        _ if super::gadgets::is_tripwire(b) => {
                            let attached = super::gadgets::wire_state(b).is_some_and(|(_, attached, _)| attached);
                            Some((DVec3::ZERO, DVec3::new(1.0, if attached { 0.09375 } else { 0.5 }, 1.0)))
                        }
                        _ => None,
                    };
                    if let Some((a, b)) = bounds {
                        let origin = p.as_dvec3();
                        if min.cmplt(origin + b).all() && max.cmpgt(origin + a).all() {
                            let c = self.redstone.contacts.entry(p).or_default();
                            c.living += living as u32;
                            c.all += 1;
                            c.arrows |= arrow;
                            c.carts += u32::from(cart);
                        }
                    }
                    if living
                        && matches!(
                            b.base(),
                            super::block::Block::REDSTONE_ORE | super::block::Block::DEEPSLATE_REDSTONE_ORE
                        )
                    {
                        self.touch_redstone_ore(p);
                    }
                }
            }
        }
    }
}

pub(super) fn plate_power(kind: u8, c: Contacts) -> u8 {
    match kind {
        0 => {
            if c.living > 0 {
                15
            } else {
                0
            }
        }
        1 => {
            if c.all > 0 {
                15
            } else {
                0
            }
        }
        2 => c.all.min(15) as u8,
        _ => c.all.min(150).div_ceil(10) as u8,
    }
}
