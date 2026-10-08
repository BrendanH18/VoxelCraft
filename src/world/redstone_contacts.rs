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
            self.redstone_contact_box(shape.aabb(pos), true, false);
        }
        for mob in &entities.mobs {
            if mob.alive() {
                self.redstone_contact_box(mob.aabb(), true, false);
            }
        }
        for item in &entities.items {
            self.redstone_contact_box((item.pos - DVec3::splat(0.125), item.pos + DVec3::splat(0.125)), false, false);
        }
        for arrow in &entities.arrows {
            self.redstone_contact_box((arrow.pos - DVec3::splat(0.05), arrow.pos + DVec3::splat(0.05)), false, true);
        }
        // Iterate by temporarily taking the retained map; no per-tick allocation.
        let contacts = std::mem::take(&mut self.redstone.contacts);
        for (&p, c) in &contacts {
            match self.get_block(p).and_then(r::component) {
                Some(Component::Plate { kind, power: 0 }) => {
                    let power = plate_power(kind, *c);
                    if power > 0 {
                        self.edit(p, r::plate(kind, power), false);
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
    }

    fn redstone_contact_box(&mut self, (min, max): (DVec3, DVec3), living: bool, arrow: bool) {
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
                            Some((DVec3::new(0.125, 0.0, 0.125), DVec3::new(0.875, 0.25, 0.875)))
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
                        _ => None,
                    };
                    if let Some((a, b)) = bounds {
                        let origin = p.as_dvec3();
                        if min.cmplt(origin + b).all() && max.cmpgt(origin + a).all() {
                            let c = self.redstone.contacts.entry(p).or_default();
                            c.living += living as u32;
                            c.all += 1;
                            c.arrows |= arrow;
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
