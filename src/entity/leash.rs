//! Java 1.21 player leads and fence knots. Anchors use stable profile ids.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{
    inventory::{Stack, StackName},
    item::Item,
    world::block::{Block, Shaped},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leash {
    Player(PlayerId),
    Fence(IVec3),
    /// Another mob, by [`Mob::uid`] (Java 26.3 `Entity.interact`: a
    /// sneaking player ties the mobs they lead to a leashable mob).
    Mob(u32),
}

/// `Leashable.LEASH_TOO_FAR_DIST`: a lead longer than this snaps.
pub const SNAP_DISTANCE: f64 = 12.0;
/// `LEASH_ELASTIC_DIST`: the spring's slack.
pub const ELASTIC_DISTANCE: f64 = 6.0;
/// `AXIS_SPECIFIC_ELASTICITY`.
const AXIS_ELASTICITY: DVec3 = DVec3::new(0.8, 0.2, 0.8);
/// `checkElasticInteractions`: the share of the relative velocity added.
const RELATIVE_VELOCITY: f64 = 0.11;
/// `TORSIONAL_ELASTICITY`.
const TORSION: f64 = 10.0;
/// `restoreLeashFromSave`: ticks a holder may stay missing before the lead drops.
pub const MISSING_HOLDER_TICKS: u16 = 100;
/// `Leashable.leashableInArea`: the cube searched around a holder.
const AREA: f64 = 16.0;
/// `LeashFenceKnotEntity`: its position is 0.375 above the fence's base,
/// and its box (`EntityType.LEASH_KNOT`) is 0.375 wide and 0.5 tall.
pub const KNOT_Y: f64 = 0.375;
const KNOT_WIDTH: f64 = 0.375;
const KNOT_HEIGHT: f64 = 0.5;

impl MobKind {
    /// The kinds Java's `canBeLeashed` can allow: every non-`Enemy` mob
    /// except ambient and water animals (bats, fish, tadpoles), pandas,
    /// turtles and villagers, plus the hostile mobs that override it
    /// (hoglins, zoglins, zombie horses, zombie nautiluses, camel husks and
    /// sulfur cubes). [`Mob::can_be_leashed`] adds the per-mob conditions.
    pub fn leashable(self) -> bool {
        matches!(
            self,
            Self::Allay
                | Self::CopperGolem
                | Self::Cow
                | Self::Mooshroom
                | Self::Frog
                | Self::PolarBear
                | Self::Sniffer
                | Self::Armadillo
                | Self::Ocelot
                | Self::Sheep
                | Self::Pig
                | Self::Chicken
                | Self::Cat
                | Self::Wolf
                | Self::Fox
                | Self::Parrot
                | Self::Rabbit
                | Self::Goat
                | Self::Bee
                | Self::Hoglin
                | Self::Zoglin
                | Self::Strider
                | Self::TraderLlama
                | Self::Horse
                | Self::Donkey
                | Self::Mule
                | Self::Llama
                | Self::Camel
                | Self::CamelHusk
                | Self::SkeletonHorse
                | Self::ZombieHorse
                | Self::IronGolem
                | Self::SnowGolem
                | Self::Dolphin
                | Self::Squid
                | Self::GlowSquid
                | Self::Axolotl
                | Self::HappyGhast
                | Self::Nautilus
                | Self::ZombieNautilus
                | Self::SulfurCube
        )
    }
}

impl Mob {
    /// Java 26.3 `canBeLeashed`: the kind, and an angry wolf, an angry
    /// nautilus, a sulfur cube without a block, or a camel husk, zombie
    /// horse or zombie nautilus carrying a mob (`isMobControlled`, from
    /// [`Entities::mob_controlled`]) refuse.
    pub fn can_be_leashed(&self, mob_controlled: bool) -> bool {
        self.kind.leashable()
            && match self.kind {
                MobKind::Wolf => self.angry_player.is_none(),
                MobKind::Nautilus => self.nautilus.as_ref().is_none_or(|s| s.anger.is_none()),
                MobKind::ZombieNautilus => self.nautilus.as_ref().is_none_or(|s| s.anger.is_none()) && !mob_controlled,
                MobKind::ZombieHorse => self.mount.as_ref().is_some_and(|s| s.tame) || !mob_controlled,
                MobKind::CamelHusk => !mob_controlled,
                MobKind::SulfurCube => self.sulfur_body().is_some(),
                _ => true,
            }
    }

    /// The centre of its box, where `leashDistanceTo` measures from.
    pub fn box_center(&self) -> DVec3 {
        self.pos + DVec3::Y * (self.shape().height * 0.5)
    }

    /// `leashSnapDistance` and `leashElasticDistance` (happy ghasts 16 and 10).
    fn leash_distances(&self) -> (f64, f64) {
        if self.kind == MobKind::HappyGhast { (16.0, 10.0) } else { (SNAP_DISTANCE, ELASTIC_DISTANCE) }
    }

    /// `supportQuadLeash`: horses, donkeys, mules, camels, skeleton and
    /// zombie horses and sniffers (llamas opt out).
    fn quad_leash(&self) -> bool {
        matches!(
            self.kind,
            MobKind::Horse
                | MobKind::Donkey
                | MobKind::Mule
                | MobKind::Camel
                | MobKind::CamelHusk
                | MobKind::SkeletonHorse
                | MobKind::ZombieHorse
                | MobKind::Sniffer
        )
    }

    /// Java's `yRot` in radians from the mob's yaw (`yaw_dir` faces
    /// `(cos, 0, sin)`; Java's facing is `(-sin yRot, 0, cos yRot)`).
    fn java_yaw(&self) -> f64 {
        f64::from(self.yaw) - std::f64::consts::FRAC_PI_2
    }
}

/// The far end of a lead this tick.
#[derive(Clone, Copy, Debug)]
pub struct Holder {
    /// `position()` (feet).
    pub pos: DVec3,
    pub width: f64,
    pub height: f64,
    /// Java `yRot` in radians.
    pub yaw: f64,
    /// `getKnownMovement`, blocks per tick.
    pub movement: DVec3,
    /// `supportQuadLeashAsHolder` (happy ghasts).
    pub quad: bool,
    /// Where the rope is drawn to.
    pub rope: DVec3,
}

impl Holder {
    pub fn center(&self) -> DVec3 {
        self.pos + DVec3::Y * (self.height * 0.5)
    }

    /// A fence knot at `p` (`LeashFenceKnotEntity`).
    pub fn knot(p: IVec3) -> Self {
        let pos = p.as_dvec3() + DVec3::new(0.5, KNOT_Y, 0.5);
        Holder {
            pos,
            width: KNOT_WIDTH,
            height: KNOT_HEIGHT,
            yaw: 0.0,
            movement: DVec3::ZERO,
            quad: false,
            rope: pos + DVec3::Y * 0.2,
        }
    }
}

/// What the leashed thing is, for the spring.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: DVec3,
    pub width: f64,
    pub height: f64,
    pub yaw: f64,
    /// Blocks per tick.
    pub movement: DVec3,
    pub quad: bool,
    pub elastic: f64,
}

/// `Leashable.checkElasticInteractions`: the change in motion (blocks per
/// tick) and in angular momentum from a taut lead, or `None` when every
/// attachment point is within the slack. A quad connection (a happy ghast
/// holding a horse or a boat) uses four shared corners at a quarter each.
pub fn elastic_pull(body: &Body, holder: &Holder) -> Option<(DVec3, f64)> {
    const ONE: [DVec3; 1] = [DVec3::new(0.0, 0.5, 0.5)];
    const HOLDER_ONE: [DVec3; 1] = [DVec3::new(0.0, 0.5, 0.0)];
    const QUAD: [DVec3; 4] = [
        DVec3::new(-0.5, 0.5, 0.5),
        DVec3::new(-0.5, 0.5, -0.5),
        DVec3::new(0.5, 0.5, -0.5),
        DVec3::new(0.5, 0.5, 0.5),
    ];
    let quad = body.quad && holder.quad;
    let (own, theirs): (&[DVec3], &[DVec3]) = if quad { (&QUAD, &QUAD) } else { (&ONE, &HOLDER_ONE) };
    // `Vec3.yRot(-yRot)`.
    let rot = |v: DVec3, yaw: f64| {
        let (s, c) = (-yaw).sin_cos();
        DVec3::new(v.x * c + v.z * s, v.y, v.z * c - v.x * s)
    };
    let mut force = DVec3::ZERO;
    let mut torque = 0.0;
    let mut any = false;
    for (a, b) in own.iter().zip(theirs) {
        let lever = rot(*a * DVec3::new(body.width, body.height, body.width), body.yaw);
        let at = body.pos + lever;
        let pivot = holder.pos + rot(*b * DVec3::new(holder.width, holder.height, holder.width), holder.yaw);
        // `computeDampenedSpringInteraction`.
        let distance = at.distance(pivot);
        if distance < body.elastic {
            continue;
        }
        let mut displacement = (pivot - at).normalize_or_zero() * (distance - body.elastic);
        torque += lever.z * displacement.x - lever.x * displacement.z;
        if body.movement.dot(displacement) >= 0.0 {
            displacement *= f64::from(0.3_f32);
        }
        force += displacement;
        any = true;
    }
    if !any {
        return None;
    }
    let scale = if quad { 0.25 } else { 1.0 };
    let (force, torque) = (force * scale, torque * scale);
    let pull = force * AXIS_ELASTICITY + (holder.movement - body.movement) * RELATIVE_VELOCITY;
    Some((pull, TORSION * torque))
}

/// Whether a block takes a lead knot (`#fences`).
pub fn fence(b: Block) -> bool {
    matches!(b.shaped(), Some(Shaped::Fence))
}

impl Entities {
    /// The holder at the far end of `leash`, `Err(true)` when it is gone
    /// for good (dead, spectating or a broken fence), `Err(false)` when it
    /// is only missing (unloaded or not yet back).
    pub(super) fn leash_holder<W: MobWorld + ?Sized>(&self, leash: Leash, w: &W, ctx: &Ctx) -> Result<Holder, bool> {
        match leash {
            Leash::Player(id) => {
                let t = ctx.players.iter().find(|t| t.id == id).ok_or(false)?;
                // `canInteractWithLevel`.
                if !t.alive || t.spectator {
                    return Err(true);
                }
                Ok(Holder {
                    pos: t.pos,
                    width: t.shape.half_width * 2.0,
                    height: t.shape.height,
                    yaw: (-t.look.x).atan2(t.look.z),
                    movement: t.vel / 20.0,
                    quad: false,
                    rope: t.pos + DVec3::Y * 1.2,
                })
            }
            Leash::Fence(p) => {
                if !w.loaded(p) {
                    return Err(false);
                }
                if !w.block(p).is_some_and(fence) {
                    return Err(true);
                }
                Ok(Holder::knot(p))
            }
            Leash::Mob(uid) => {
                let m = self.mobs.iter().find(|m| m.uid == uid).ok_or(false)?;
                if !m.alive() {
                    return Err(true);
                }
                Ok(Holder {
                    pos: m.pos,
                    width: m.shape().half_width * 2.0,
                    height: m.shape().height,
                    yaw: m.java_yaw(),
                    movement: m.vel / 20.0,
                    quad: m.kind == MobKind::HappyGhast,
                    rope: m.pos + DVec3::Y * (m.shape().height * 0.85),
                })
            }
        }
    }

    /// `Leashable.dropLeash`: the lead comes off and, with entity drops on
    /// and `drop` set, falls as an item where the mob is.
    pub(super) fn drop_leash(&mut self, index: usize, drop: bool) {
        let m = &mut self.mobs[index];
        if m.leash.take().is_none() {
            return;
        }
        m.leash_pos = None;
        m.leash_wait = 0;
        m.leash_spin = 0.0;
        let pos = m.pos;
        if drop && self.entity_drops {
            self.scatter(Stack::new(Item::LEAD, 1), pos);
        }
    }

    /// The living leashable mobs `holder` leads within `leashableInArea`'s
    /// 32-block cube around `at`.
    fn led_by(&self, holder: Leash, at: DVec3) -> Vec<usize> {
        (0..self.mobs.len())
            .filter(|&i| {
                let m = &self.mobs[i];
                m.alive() && m.leash == Some(holder) && (m.box_center() - at).abs().max_element() <= AREA
            })
            .collect()
    }

    /// Java 26.3 `Mob.interact` before `mobInteract`: a name tag names the
    /// mob, then `Entity.interact`'s leash rules. Any use by the player
    /// leading it takes the lead off (dropped unless the player has
    /// infinite materials); a lead ties it to the player, taking it from a
    /// fence or another mob first (that lead drops). `Some(true)` uses the
    /// held item, `Some(false)` acts without it.
    pub(super) fn use_lead_or_name(
        &mut self,
        index: usize,
        held: Option<Stack>,
        player: PlayerId,
        infinite: bool,
    ) -> Option<bool> {
        let controlled = self.mob_controlled(index);
        let m = self.mobs.get_mut(index)?;
        if !m.alive() {
            return None;
        }
        if let Some(held) = held
            && held.item == Item::NAME_TAG
        {
            held.name.as_str()?;
            m.name = held.name;
            m.persistent = true;
            super::illager::named(m);
            return Some(true);
        }
        if m.leash == Some(Leash::Player(player)) {
            self.drop_leash(index, !infinite);
            return Some(false);
        }
        if held.is_some_and(|s| s.item == Item::LEAD) && m.can_be_leashed(controlled) {
            if m.leash.is_some() {
                self.drop_leash(index, true);
            }
            let m = &mut self.mobs[index];
            m.leash = Some(Leash::Player(player));
            m.persistent = true;
            if let Some(a) = &mut m.animal {
                a.perched = None;
            }
            return Some(true);
        }
        None
    }

    /// Java's `isMobControlled`: a living mob rides mob `index` (mob riders
    /// name their vehicle as `MOB_VEHICLE | uid`; a player aboard, in
    /// either seat, isn't a mob).
    pub fn mob_controlled(&self, index: usize) -> bool {
        let Some(uid) = self.mobs.get(index).map(|m| m.uid).filter(|&u| u != 0) else { return false };
        let vehicle = super::mounts::MOB_VEHICLE | uid;
        self.mobs.iter().any(|r| r.alive() && r.riding == Some(vehicle))
    }

    /// `Entity.interact` with a sneaking player: the mobs they lead within
    /// 12 blocks of this leashable, grown mob are tied to it instead.
    pub fn tie_led_mobs_to(&mut self, index: usize, player: PlayerId) -> bool {
        let Some(target) = self.mobs.get(index) else { return false };
        if !target.alive() || target.baby || !target.can_be_leashed(self.mob_controlled(index)) {
            return false;
        }
        self.ensure_mob_ids();
        let (uid, center) = (self.mobs[index].uid, self.mobs[index].box_center());
        let mut tied = false;
        for i in self.led_by(Leash::Player(player), center) {
            if i != index && self.mobs[i].box_center().distance(center) <= self.mobs[i].leash_distances().0 {
                self.mobs[i].leash = Some(Leash::Mob(uid));
                tied = true;
            }
        }
        tied
    }

    /// Shears on a mob (`shearOffAllLeashConnections`): its own lead and
    /// every lead tied to it come off and drop.
    pub fn shear_leashes(&mut self, index: usize) -> bool {
        let Some(m) = self.mobs.get(index) else { return false };
        let (uid, center, leashed) = (m.uid, m.box_center(), m.leash.is_some());
        let tied = if uid == 0 { Vec::new() } else { self.led_by(Leash::Mob(uid), center) };
        if leashed {
            self.drop_leash(index, true);
        }
        for &i in &tied {
            self.drop_leash(i, true);
        }
        leashed || !tied.is_empty()
    }

    /// Using a fence (`FenceBlock.useWithoutItem`, `LeadItem.useOn`) or its
    /// knot: the player's mobs within 12 blocks of the knot are tied to it
    /// (`bindPlayerMobs`); failing that, a knot's mobs within 12 blocks of
    /// the player go back to the player unless they are sneaking
    /// (`LeashFenceKnotEntity.interact`). Nothing is dropped.
    pub fn use_fence_knot_by(&mut self, player: PlayerId, at: DVec3, sneaking: bool, p: IVec3) -> bool {
        let knot = Holder::knot(p).center();
        let mut tied = self.tie_boats_to_knot(player, p);
        for i in self.led_by(Leash::Player(player), knot) {
            if self.mobs[i].box_center().distance(knot) <= self.mobs[i].leash_distances().0 {
                self.mobs[i].leash = Some(Leash::Fence(p));
                tied = true;
            }
        }
        if tied || sneaking || !self.has_fence_knot(p) {
            return tied;
        }
        let mut taken = false;
        for i in self.boats_led_by(Leash::Fence(p), knot) {
            if self.boat_center(i).distance(at) <= SNAP_DISTANCE {
                self.minecarts[i].leash = Some(Leash::Player(player));
                taken = true;
            }
        }
        for i in self.led_by(Leash::Fence(p), knot) {
            if self.mobs[i].box_center().distance(at) <= self.mobs[i].leash_distances().0 {
                self.mobs[i].leash = Some(Leash::Player(player));
                taken = true;
            }
        }
        taken
    }

    /// [`Entities::use_fence_knot_by`] for a player standing at the fence.
    pub fn use_fence_knot(&mut self, player: PlayerId, p: IVec3) -> bool {
        self.use_fence_knot_by(player, p.as_dvec3() + DVec3::splat(0.5), false, p)
    }

    /// Whether a mob is tied to a knot on fence `p`.
    pub fn has_fence_knot(&self, p: IVec3) -> bool {
        self.mobs.iter().any(|m| m.alive() && m.leash == Some(Leash::Fence(p)))
            || self.minecarts.iter().any(|c| c.leash == Some(Leash::Fence(p)))
    }

    /// The knot on `p` is broken or sheared: every mob tied to it drops its
    /// lead where it stands (the knot itself drops nothing).
    pub fn break_fence_knot(&mut self, p: IVec3) -> bool {
        let tied: Vec<usize> = (0..self.mobs.len()).filter(|&i| self.mobs[i].leash == Some(Leash::Fence(p))).collect();
        for &i in &tied {
            self.drop_leash(i, true);
        }
        let boats: Vec<usize> =
            (0..self.minecarts.len()).filter(|&i| self.minecarts[i].leash == Some(Leash::Fence(p))).collect();
        for &i in &boats {
            self.drop_boat_leash(i, true);
        }
        !tied.is_empty() || !boats.is_empty()
    }

    /// `Leashable.tickLeash` for every leashed mob: a lead whose holder is
    /// gone drops at once, one whose holder is missing after 100 ticks; past
    /// the snap distance (12, centre to centre) it breaks; beyond the
    /// elastic distance (6 less both widths) the spring pulls and twists
    /// the mob; nearer, `PathfinderMob.closeRangeLeashBehaviour` walks it
    /// toward the holder ([`Mob::leash_think`]).
    pub(super) fn tick_leashes<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W, ctx: &Ctx) {
        let ticks = f64::from(dt) * 20.0;
        for i in 0..self.mobs.len() {
            let Some(leash) = self.mobs[i].leash else { continue };
            if !w.entity_ticking(self.mobs[i].pos.floor().as_ivec3()) {
                continue;
            }
            if !self.mobs[i].alive() {
                self.drop_leash(i, true);
                continue;
            }
            let holder = match self.leash_holder(leash, w, ctx) {
                Ok(h) => h,
                Err(gone) => {
                    let m = &mut self.mobs[i];
                    m.leash_pos = None;
                    m.leash_wait = m.leash_wait.saturating_add(1);
                    if gone || m.leash_wait > MISSING_HOLDER_TICKS {
                        self.drop_leash(i, true);
                    }
                    continue;
                }
            };
            let m = &mut self.mobs[i];
            m.leash_wait = 0;
            m.leash_pos = Some(holder.rope);
            let (snap, elastic) = m.leash_distances();
            let distance = m.box_center().distance(holder.center());
            m.leash_close = false;
            if distance > snap {
                self.drop_leash(i, true);
                continue;
            }
            let body = Body {
                pos: m.pos,
                width: m.shape().half_width * 2.0,
                height: m.shape().height,
                yaw: m.java_yaw(),
                movement: m.vel / 20.0,
                quad: m.quad_leash(),
                elastic,
            };
            let pull = (distance > elastic - holder.width - body.width).then(|| elastic_pull(&body, &holder)).flatten();
            if let Some((pull, spin)) = pull {
                m.vel += pull * 20.0 * ticks;
                m.leash_spin += spin;
                // `onElasticLeashPull` (v1.0 gap Track 3b: a seated camel stands).
                m.camel_leash_pull(w);
            } else {
                m.leash_close = true;
            }
            // `setYRot(getYRot() - angularMomentum)`, then friction.
            m.yaw -= m.leash_spin.to_radians() as f32;
            let friction = if m.on_ground {
                f64::from(crate::player::friction(w.block(m.pos.floor().as_ivec3() - IVec3::Y)) * 0.91)
            } else if m.in_water {
                0.8
            } else {
                0.91
            };
            m.leash_spin *= friction;
        }
    }
}
impl Mob {
    /// `PathfinderMob.closeRangeLeashBehaviour`: within the elastic reach a
    /// leashed mob that isn't panicking walks to two blocks short of its
    /// holder at `followLeashSpeed` (llamas 2, others 1). Allays and happy
    /// ghasts don't (`shouldStayCloseToLeashHolder`); a sitting pet stays put.
    pub(super) fn leash_think<W: MobWorld + ?Sized>(
        &mut self,
        _dt: f32,
        w: &W,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let at = self.leash_pos?;
        if self.animal.as_ref().is_some_and(|a| a.sit_pose) {
            return Some((None, 0.0));
        }
        // A trader llama follows its wandering trader on the merchant's lead.
        if self.leash.is_none() {
            let delta = (at - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            if delta.length_squared() < 4.0 {
                return None;
            }
            self.look_at(at, 1.2);
            return Some((self.pursue(w, at, 2.5, true, false, rng), 2.5));
        }
        if !self.leash_close
            || matches!(self.kind, MobKind::Allay | MobKind::HappyGhast)
            || self.ai == super::mob::Ai::Panic
        {
            return None;
        }
        let delta = at - self.pos;
        let distance = delta.length();
        if distance <= 2.0 {
            return None;
        }
        let goal = self.pos + delta / distance * (distance - 2.0);
        let speed = self.goal_speed(if matches!(self.kind, MobKind::Llama | MobKind::TraderLlama) { 2.0 } else { 1.0 });
        self.look_at(at, 1.2);
        Some((self.pursue(w, goal, speed, true, false, rng), speed))
    }
}
pub(super) fn save(m: &Mob) -> Value {
    json!({"name":m.name.as_str(),"leash":save_leash(m.leash)})
}
pub(super) fn save_leash(leash: Option<Leash>) -> Value {
    match leash {
        Some(Leash::Player(id)) => json!({"player":id.0}),
        Some(Leash::Fence(p)) => json!({"fence":p.to_array()}),
        Some(Leash::Mob(uid)) => json!({"mob":uid}),
        None => Value::Null,
    }
}
pub(super) fn load_leash(v: &Value) -> Option<Leash> {
    let id = |v: &Value| v.as_u64().and_then(|id| u32::try_from(id).ok());
    id(&v["player"])
        .map(|id| Leash::Player(PlayerId(id)))
        .or_else(|| id(&v["mob"]).filter(|&u| u != 0).map(Leash::Mob))
        .or_else(|| {
            let a = v["fence"].as_array()?;
            if a.len() != 3 {
                return None;
            }
            Some(Leash::Fence(IVec3::new(
                i32::try_from(a[0].as_i64()?).ok()?,
                i32::try_from(a[1].as_i64()?).ok()?,
                i32::try_from(a[2].as_i64()?).ok()?,
            )))
        })
}
/// A lead in a cart line's comma-free form: `p<player>`, `m<uid>` or
/// `f<x>:<y>:<z>`.
pub(super) fn leash_code(leash: Leash) -> String {
    match leash {
        Leash::Player(id) => format!("p{}", id.0),
        Leash::Mob(uid) => format!("m{uid}"),
        Leash::Fence(p) => format!("f{}:{}:{}", p.x, p.y, p.z),
    }
}
pub(super) fn leash_from_code(code: &str) -> Option<Leash> {
    let (tag, rest) = code.split_at_checked(1)?;
    match tag {
        "p" => Some(Leash::Player(PlayerId(rest.parse().ok()?))),
        "m" => Some(Leash::Mob(rest.parse().ok().filter(|&u: &u32| u != 0)?)),
        "f" => {
            let mut c = rest.split(':').map(|n| n.parse::<i32>().ok());
            let p = IVec3::new(c.next()??, c.next()??, c.next()??);
            c.next().is_none().then_some(Leash::Fence(p))
        }
        _ => None,
    }
}
pub(super) fn load(m: &mut Mob, v: &Value) {
    m.name = v["name"].as_str().and_then(StackName::new).unwrap_or_default();
    m.leash = load_leash(&v["leash"]);
    if m.name.as_str().is_some() || m.leash.is_some() {
        m.persistent = true;
    }
}

mod boats;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        physics::{BlockSource, test_util::Grid},
        world::terrain::Dimension,
    };
    struct Flat(Grid);
    impl BlockSource for Flat {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Flat {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
    }
    fn ctx(at: DVec3) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![super::super::Target::new(PlayerId(3), at, false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    fn leads(e: &Entities) -> u8 {
        e.items.iter().filter(|i| i.stack.item == Item::LEAD).map(|i| i.stack.count).sum()
    }

    #[test]
    fn leads_attach_tie_to_fences_and_drop_when_the_knot_breaks() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(2.0, 1.0, 0.0));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3)), Some(true));
        assert_eq!(e.mobs[0].leash, Some(Leash::Player(PlayerId(3))));
        let post = IVec3::new(0, 1, 0);
        assert!(e.use_fence_knot(PlayerId(3), post));
        assert_eq!(e.mobs[0].leash, Some(Leash::Fence(post)));
        assert!(e.break_fence_knot(post));
        assert_eq!(e.mobs[0].leash, None);
        assert_eq!(leads(&e), 1);
    }

    #[test]
    fn leads_snap_past_twelve_blocks_and_pull_beyond_six() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(0.0, 1.0, 0.0));
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        let w = Flat(Grid::flat(0));
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(8.0, 1.0, 0.0)));
        assert!(e.mobs[0].leash.is_some());
        assert!(e.mobs[0].vel.x > 0.0, "a taut lead pulls toward its holder");
        e.mobs[0].vel = DVec3::ZERO;
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(11.5, 1.0, 0.0)));
        assert!(e.mobs[0].leash.is_some(), "12 blocks, centre to centre, before it snaps");
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(12.5, 1.0, 0.0)));
        assert_eq!(e.mobs[0].leash, None);
        assert_eq!(leads(&e), 1);
    }

    /// `computeDampenedSpringInteraction`: displacement past the slack,
    /// a third (0.3) when the leashee already moves that way, torque from
    /// the lever arm, then `AXIS_SPECIFIC_ELASTICITY` and 0.11 of the
    /// relative velocity.
    #[test]
    fn the_spring_follows_check_elastic_interactions() {
        let body = Body {
            pos: DVec3::ZERO,
            width: 0.9,
            height: 0.9,
            yaw: 0.0,
            movement: DVec3::ZERO,
            quad: false,
            elastic: ELASTIC_DISTANCE,
        };
        let holder = Holder {
            pos: DVec3::new(10.0, 0.0, 0.0),
            width: 0.6,
            height: 1.8,
            yaw: 0.0,
            movement: DVec3::ZERO,
            quad: false,
            rope: DVec3::ZERO,
        };
        let (pull, spin) = elastic_pull(&body, &holder).unwrap();
        let at = DVec3::new(0.0, 0.45, 0.45);
        let pivot = DVec3::new(10.0, 0.9, 0.0);
        let d = at.distance(pivot);
        let disp = (pivot - at) / d * (d - 6.0);
        let expected = disp * f64::from(0.3_f32) * DVec3::new(0.8, 0.2, 0.8);
        assert!((pull - expected).length() < 1e-9, "{pull} vs {expected}");
        assert!((spin - 10.0 * (0.45 * disp.x)).abs() < 1e-9);
        // Moving away from the holder: no damping, and 0.11 of the
        // relative velocity pulls back.
        let away = Body { movement: DVec3::new(-0.2, 0.0, 0.0), ..body };
        let (pull, _) = elastic_pull(&away, &holder).unwrap();
        let expected = disp * DVec3::new(0.8, 0.2, 0.8) + DVec3::new(0.2, 0.0, 0.0) * 0.11;
        assert!((pull - expected).length() < 1e-9);
        // Within the slack nothing happens.
        let near = Holder { pos: DVec3::new(4.0, 0.0, 0.0), ..holder };
        assert!(elastic_pull(&body, &near).is_none());
    }

    /// `LeashFenceKnotEntity.interact`: with nothing to tie, a knot's mobs
    /// go back to the player who uses it (no lead drops), unless sneaking.
    #[test]
    fn knots_give_their_mobs_back_unless_the_player_sneaks() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(2.0, 1.0, 0.0));
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        let post = IVec3::new(0, 1, 0);
        let me = DVec3::new(1.0, 1.9, 1.0);
        assert!(e.use_fence_knot_by(PlayerId(3), me, false, post));
        assert_eq!(e.mobs[0].leash, Some(Leash::Fence(post)));
        assert!(!e.use_fence_knot_by(PlayerId(3), me, true, post), "sneaking leaves the knot alone");
        assert!(e.use_fence_knot_by(PlayerId(3), me, false, post));
        assert_eq!(e.mobs[0].leash, Some(Leash::Player(PlayerId(3))));
        assert_eq!(leads(&e), 0);
        // With no mobs and no knot, the fence does nothing.
        assert!(!e.use_fence_knot_by(PlayerId(4), me, false, IVec3::new(5, 1, 5)));
    }

    /// `Entity.interact`: the player leading a mob takes the lead off with
    /// any item (dropped, unless they have infinite materials); a lead
    /// moves a fence-tied mob to the player and drops the old lead.
    #[test]
    fn the_leading_player_unties_with_any_item_and_a_lead_moves_a_tied_mob() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(2.0, 1.0, 0.0));
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::WHEAT, 1)), PlayerId(3)), Some(false));
        assert_eq!(e.mobs[0].leash, None, "wheat doesn't feed a cow its holder clicks");
        assert_eq!(leads(&e), 1);
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        assert_eq!(e.use_animal_with_materials(0, None, PlayerId(3), true), Some(false));
        assert_eq!(leads(&e), 1, "creative keeps the lead");
        e.mobs[0].leash = Some(Leash::Fence(IVec3::new(0, 1, 0)));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(5)), Some(true));
        assert_eq!(e.mobs[0].leash, Some(Leash::Player(PlayerId(5))));
        assert_eq!(leads(&e), 2, "the knot's lead dropped");
    }

    /// A sneaking player ties the mobs they lead to another leashable,
    /// grown mob; shears on that mob cut every lead on it.
    #[test]
    fn sneaking_ties_led_mobs_to_a_mob_and_shears_cut_them() {
        let mut e = Entities::new(1);
        for x in [0.0, 1.5] {
            e.spawn(MobKind::Sheep, DVec3::new(x, 1.0, 0.0));
        }
        e.spawn(MobKind::Horse, DVec3::new(4.0, 1.0, 0.0));
        e.spawn(MobKind::Horse, DVec3::new(4.0, 1.0, 3.0));
        e.ensure_mob_ids();
        for i in 0..2 {
            e.use_animal(i, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        }
        e.mobs[3].baby = true;
        assert!(!e.tie_led_mobs_to(3, PlayerId(3)), "not to a baby");
        assert!(e.tie_led_mobs_to(2, PlayerId(3)));
        let horse = e.mobs[2].uid;
        assert!(e.mobs[..2].iter().all(|m| m.leash == Some(Leash::Mob(horse))));
        let w = Flat(Grid::flat(0));
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(30.0, 1.0, 0.0)));
        assert!(e.mobs[..2].iter().all(|m| m.leash == Some(Leash::Mob(horse))), "held by the horse");
        assert!(e.shear_leashes(2));
        assert!(e.mobs.iter().all(|m| m.leash.is_none()));
        assert_eq!(leads(&e), 2);
        // A dead holder drops the lead at once.
        e.use_animal(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3));
        e.tie_led_mobs_to(2, PlayerId(3));
        e.mobs[2].health = 0.0;
        e.mobs[2].dying = Some(0.0);
        e.tick_leashes(0.05, &w, &ctx(DVec3::new(30.0, 1.0, 0.0)));
        assert_eq!(e.mobs[0].leash, None);
    }

    /// `restoreLeashFromSave`: a lead whose holder never comes back drops
    /// once the mob has waited 100 ticks.
    #[test]
    fn a_missing_holder_drops_the_lead_after_a_hundred_ticks() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(2.0, 1.0, 0.0));
        e.mobs[0].leash = Some(Leash::Player(PlayerId(9)));
        let w = Flat(Grid::flat(0));
        let c = ctx(DVec3::new(2.0, 1.0, 3.0));
        for _ in 0..MISSING_HOLDER_TICKS {
            e.tick_leashes(0.05, &w, &c);
        }
        assert!(e.mobs[0].leash.is_some());
        e.tick_leashes(0.05, &w, &c);
        assert_eq!(e.mobs[0].leash, None);
        assert_eq!(leads(&e), 1);
    }

    /// Java 26.3 `canBeLeashed`.
    #[test]
    fn leashability_follows_can_be_leashed() {
        let mut e = Entities::new(1);
        for kind in [MobKind::Bee, MobKind::Squid, MobKind::HappyGhast, MobKind::SkeletonHorse, MobKind::Hoglin] {
            e.spawn(kind, DVec3::Y);
            assert!(e.mobs.last().unwrap().can_be_leashed(false), "{kind:?}");
        }
        for kind in [MobKind::Villager, MobKind::Panda, MobKind::Turtle, MobKind::Bat, MobKind::Cod, MobKind::Zombie] {
            e.spawn(kind, DVec3::Y);
            assert!(!e.mobs.last().unwrap().can_be_leashed(false), "{kind:?}");
        }
        e.spawn(MobKind::Wolf, DVec3::Y);
        e.mobs.last_mut().unwrap().angry_player = Some(PlayerId(1));
        assert!(!e.mobs.last().unwrap().can_be_leashed(false), "an angry wolf");
    }

    /// Review fix N2: a camel husk carrying a husk refuses a lead
    /// (`isMobControlled`), and one carrying a second player doesn't.
    #[test]
    fn a_mount_refuses_a_lead_while_a_mob_rides_it() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::CamelHusk, DVec3::Y);
        e.spawn(MobKind::Husk, DVec3::Y);
        e.ensure_mob_ids();
        let lead = Some(Stack::new(Item::LEAD, 1));
        e.mobs[0].mount.as_mut().unwrap().passenger = Some(PlayerId(2));
        assert!(!e.mob_controlled(0), "a player in the rear seat doesn't steer it");
        assert!(e.mobs[0].can_be_leashed(e.mob_controlled(0)));
        e.mobs[0].mount.as_mut().unwrap().passenger = None;
        e.mobs[1].riding = Some(crate::entity::mounts::MOB_VEHICLE | e.mobs[0].uid);
        assert!(e.mob_controlled(0));
        assert_eq!(e.use_lead_or_name(0, lead, PlayerId::HOST, false), None, "the husk steers it");
        assert_eq!(e.mobs[0].leash, None);
        e.mobs[1].riding = None;
        assert_eq!(e.use_lead_or_name(0, lead, PlayerId::HOST, false), Some(true), "its rider got off");
    }

    /// Boats take leads, are pulled by them, snap at 12 and save them.
    #[test]
    fn boats_take_leads_and_keep_them_in_saves() {
        use crate::entity::minecart::{CartKind, Minecart};
        let mut e = Entities::new(1);
        e.minecarts.push(Minecart::new(1, CartKind::Boat { wood: 0, chest: false }, DVec3::new(0.5, 1.0, 0.5)));
        let eye = DVec3::new(0.5, 1.4, -2.5);
        let lead = Some(Item::LEAD);
        assert_eq!(e.use_boat_leash(eye, DVec3::Z, 5.0, PlayerId(3), None, false), None, "boarding instead");
        assert_eq!(e.use_boat_leash(eye, DVec3::Z, 5.0, PlayerId(3), lead, false), Some(true));
        assert_eq!(e.minecarts[0].leash, Some(Leash::Player(PlayerId(3))));
        let line = e.minecarts[0].serialize();
        let back = Minecart::deserialize(&line).unwrap();
        assert_eq!(back.leash, Some(Leash::Player(PlayerId(3))));
        let w = Flat(Grid::flat(0));
        e.tick_boat_leashes(0.05, &w, &ctx(DVec3::new(9.5, 1.0, 0.5)));
        assert!(e.minecarts[0].vel.x > 0.0, "towed toward the player");
        assert!(e.use_fence_knot_by(PlayerId(3), DVec3::new(1.0, 1.9, 1.0), false, IVec3::new(2, 1, 2)));
        assert_eq!(e.minecarts[0].leash, Some(Leash::Fence(IVec3::new(2, 1, 2))));
        let back = Minecart::deserialize(&e.minecarts[0].serialize()).unwrap();
        assert_eq!(back.leash, Some(Leash::Fence(IVec3::new(2, 1, 2))));
        e.minecarts[0].leash = Some(Leash::Player(PlayerId(3)));
        e.tick_boat_leashes(0.05, &w, &ctx(DVec3::new(20.5, 1.0, 0.5)));
        assert_eq!(e.minecarts[0].leash, None);
        assert_eq!(leads(&e), 1);
        // A boat line from before leads still loads, without one.
        let mut plain = Minecart::new(2, CartKind::Boat { wood: 0, chest: false }, DVec3::ZERO);
        plain.leash = None;
        assert_eq!(Minecart::deserialize(&plain.serialize()).unwrap().leash, None);
        assert!(Minecart::deserialize(&(plain.serialize() + ",x7")).is_none());
    }

    #[test]
    fn mob_leashes_save_by_uid() {
        let mut m = Mob::new(MobKind::Cow, DVec3::Y, 0.0);
        m.leash = Some(Leash::Mob(77));
        let mut back = Mob::new(MobKind::Cow, DVec3::Y, 0.0);
        load(&mut back, &save(&m));
        assert_eq!(back.leash, Some(Leash::Mob(77)));
        assert!(back.persistent);
        assert_eq!(
            leash_from_code(&leash_code(Leash::Fence(IVec3::new(-4, 70, 9)))),
            Some(Leash::Fence(IVec3::new(-4, 70, 9)))
        );
        assert_eq!(leash_from_code("m0"), None);
    }

    #[test]
    fn hostile_mobs_cannot_be_leashed() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Zombie, DVec3::Y);
        assert_eq!(e.use_lead_or_name(0, Some(Stack::new(Item::LEAD, 1)), PlayerId(3), false), None);
    }

    #[test]
    fn name_tags_need_a_name_and_names_survive_saves() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::Y);
        assert_eq!(e.use_lead_or_name(0, Some(Stack::new(Item::NAME_TAG, 1)), PlayerId(3), false), None);
        let tag = Stack { name: StackName::new("Bessie").unwrap(), ..Stack::new(Item::NAME_TAG, 1) };
        assert_eq!(e.use_lead_or_name(0, Some(tag), PlayerId(3), false), Some(true));
        assert!(e.mobs[0].persistent);
        e.mobs[0].leash = Some(Leash::Fence(IVec3::new(4, 5, -6)));
        let mut m = Mob::new(MobKind::Cow, DVec3::Y, 0.0);
        load(&mut m, &save(&e.mobs[0]));
        assert_eq!(m.name.as_str(), Some("Bessie"));
        assert_eq!(m.leash, Some(Leash::Fence(IVec3::new(4, 5, -6))));
        assert!(m.persistent);
    }
}
