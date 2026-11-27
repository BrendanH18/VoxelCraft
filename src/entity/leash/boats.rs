//! v1.0 Track 6: boats on leads. Java 26.3's `AbstractBoat` is
//! `Leashable`: a player leads one with a lead, ties it to a fence, and a
//! happy ghast tows it by four corners. Its lead runs the same tick as a
//! mob's (`Leashable.tickLeash`), with no walking toward the holder.
use super::{Body, Holder, Leash, elastic_pull};
use crate::entity::{Ctx, Entities, MobWorld, PlayerId};
use crate::inventory::Stack;
use crate::item::Item;
use glam::{DVec3, IVec3};

impl Entities {
    /// A lead used on the boat under the crosshair (`Entity.interact`
    /// before `AbstractBoat.interact` boards it): the player leading it
    /// takes the lead off (dropped unless `infinite`); a lead ties it to the
    /// player, taking it from a fence first. `Some(true)` uses the lead,
    /// `Some(false)` acts without it, `None` leaves the boat to boarding.
    pub fn use_boat_leash(
        &mut self,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
        player: PlayerId,
        held: Option<Item>,
        infinite: bool,
    ) -> Option<bool> {
        let i = self.cart_ray(eye, dir, reach)?;
        let cart = &self.minecarts[i];
        cart.kind.boat()?;
        if cart.leash == Some(Leash::Player(player)) {
            self.drop_boat_leash(i, !infinite);
            return Some(false);
        }
        if held != Some(Item::LEAD) {
            return None;
        }
        if cart.leash.is_some() {
            self.drop_boat_leash(i, true);
        }
        self.minecarts[i].leash = Some(Leash::Player(player));
        Some(true)
    }

    pub(super) fn drop_boat_leash(&mut self, index: usize, drop: bool) {
        let cart = &mut self.minecarts[index];
        if cart.leash.take().is_none() {
            return;
        }
        cart.leash_wait = 0;
        cart.leash_spin = 0.0;
        let pos = cart.pos;
        if drop && self.entity_drops {
            self.scatter(Stack::new(Item::LEAD, 1), pos);
        }
    }

    /// Boats `holder` leads within the 32-block cube around `at`.
    pub(super) fn boats_led_by(&self, holder: Leash, at: DVec3) -> Vec<usize> {
        (0..self.minecarts.len())
            .filter(|&i| {
                let c = &self.minecarts[i];
                c.leash == Some(holder) && (c.pos - at).abs().max_element() <= super::AREA
            })
            .collect()
    }

    /// A boat's centre (its box is 1.375 wide and 0.5625 tall).
    pub(super) fn boat_center(&self, index: usize) -> DVec3 {
        let c = &self.minecarts[index];
        c.pos + DVec3::Y * (c.shape().height * 0.5)
    }

    /// `tickLeash` for boats: the lead drops when its holder is gone (or
    /// missing for 100 ticks) or past 12 blocks; beyond the elastic reach
    /// the spring pulls the boat (velocity in blocks a tick) and turns it.
    pub(crate) fn tick_boat_leashes<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W, ctx: &Ctx) {
        let ticks = f64::from(dt) * 20.0;
        for i in 0..self.minecarts.len() {
            let Some(leash) = self.minecarts[i].leash else { continue };
            if !w.entity_ticking(self.minecarts[i].pos.floor().as_ivec3()) {
                continue;
            }
            let holder: Holder = match self.leash_holder(leash, w, ctx) {
                Ok(h) => h,
                Err(gone) => {
                    let c = &mut self.minecarts[i];
                    c.leash_wait = c.leash_wait.saturating_add(1);
                    if gone || c.leash_wait > super::MISSING_HOLDER_TICKS {
                        self.drop_boat_leash(i, true);
                    }
                    continue;
                }
            };
            let center = self.boat_center(i);
            let c = &mut self.minecarts[i];
            c.leash_wait = 0;
            if center.distance(holder.center()) > super::SNAP_DISTANCE {
                self.drop_boat_leash(i, true);
                continue;
            }
            let shape = c.shape();
            let body = Body {
                pos: c.pos,
                width: shape.half_width * 2.0,
                height: shape.height,
                yaw: f64::from(c.yaw) - std::f64::consts::FRAC_PI_2,
                movement: c.vel,
                quad: true,
                elastic: super::ELASTIC_DISTANCE,
            };
            if center.distance(holder.center()) > super::ELASTIC_DISTANCE - holder.width - body.width
                && let Some((pull, spin)) = elastic_pull(&body, &holder)
            {
                c.vel += pull * ticks;
                c.leash_spin += spin;
            }
            c.yaw -= c.leash_spin.to_radians() as f32;
            let in_liquid = w.block(c.pos.floor().as_ivec3()).is_some_and(|b| b.is_fluid());
            c.leash_spin *= if c.on_ground {
                f64::from(crate::player::friction(w.block(c.pos.floor().as_ivec3() - IVec3::Y)) * 0.91)
            } else if in_liquid {
                0.8
            } else {
                0.91
            };
        }
    }

    /// Ties the boats `player` leads within 12 blocks of the knot on `p`.
    pub(super) fn tie_boats_to_knot(&mut self, player: PlayerId, p: IVec3) -> bool {
        let knot = Holder::knot(p).center();
        let mut tied = false;
        for i in self.boats_led_by(Leash::Player(player), knot) {
            if self.boat_center(i).distance(knot) <= super::SNAP_DISTANCE {
                self.minecarts[i].leash = Some(Leash::Fence(p));
                tied = true;
            }
        }
        tied
    }
}
