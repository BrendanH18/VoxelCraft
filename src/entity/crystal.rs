//! v0.16 Track G: End crystals, after Java 26.3's `EndCrystal` and
//! `EndCrystalItem`. The fight's pillar crystals and the ones players place
//! are the same entity: any hit but the dragon's blows one up (power 6,
//! breaking blocks, no fire), blasts destroy them without a second blast,
//! they keep a fire lit under themselves in the End, heal a dragon within
//! reach, and can carry a beam to a block (the respawn ritual's). Placed
//! crystals are saved with their dimension.

use glam::{DVec3, IVec3};

use super::{EntityEvent, PlayerId};
use crate::item::Item;
use crate::physics::{self, BlockSource};
use crate::world::block::Block;

/// The `end_crystal` item (v0.16 Track G's item 1750).
pub const END_CRYSTAL: Item = Item(1750);
/// Its icon's texture layer (Track G's layers 1029–1048).
pub const ICON_TEX: u16 = 1029;
pub const TEX_END: u16 = ICON_TEX + 1;
/// `EndCrystal.hurtServer`: `level.explode(this, .., 6.0F, false, BLOCK)`.
pub const POWER: f32 = 6.0;
/// `EntityTypes.END_CRYSTAL`: `sized(2.0F, 2.0F)`.
pub const WIDTH: f64 = 2.0;
pub const HEIGHT: f64 = 2.0;

/// One End crystal.
#[derive(Clone, Debug, PartialEq)]
pub struct Crystal {
    /// Stable while the crystal exists (not saved).
    pub id: u32,
    /// Feet, block-centred.
    pub pos: DVec3,
    /// Java's `time`: ticks alive, for the spin and bob.
    pub age: u32,
    /// `DATA_BEAM_TARGET`: the block its beam points at.
    pub beam: Option<IVec3>,
    /// `DATA_SHOW_BOTTOM`: the bedrock plate under it (pillar crystals);
    /// placed crystals hide it.
    pub show_bottom: bool,
    /// `setPermanentlyInvulnerable`: the respawn ritual's new pillar
    /// crystals until the dragon appears.
    pub invulnerable: bool,
}

impl Crystal {
    /// The 2x2x2 box.
    pub fn aabb(&self) -> (DVec3, DVec3) {
        let h = WIDTH / 2.0;
        (self.pos - DVec3::new(h, 0.0, h), self.pos + DVec3::new(h, HEIGHT, h))
    }

    pub fn center(&self) -> DVec3 {
        self.pos + DVec3::Y * (HEIGHT / 2.0)
    }

    /// Java's `blockPosition`.
    pub fn cell(&self) -> IVec3 {
        self.pos.floor().as_ivec3()
    }
}

/// What hurt a crystal (the parts of Java's `DamageSource` it reads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blow {
    /// A player's melee hit or projectile. A creative player's hit gets
    /// through the ritual crystals' invulnerability (`isCreativePlayer`).
    Player { id: PlayerId, creative: bool },
    /// v1.0 gap Track 8b: a mob's projectile or blow: the blast names the
    /// mob (`explosion(crystal, source.getEntity())`).
    Mob(super::MobKind),
    /// Anything else that isn't a blast: a projectile with no owner.
    Other,
    /// A blast (`#is_explosion`): the crystal goes without a blast of its own.
    Explosion,
}

impl Blow {
    fn player(self) -> Option<PlayerId> {
        match self {
            Blow::Player { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// v1.0 gap Track 8b: a crystal's blast that a mob set off is
/// `explosion(crystal, mob)`, `PLAYER_EXPLOSION` (`explosion.player`: "was
/// blown up by" the mob) for the mobs whose projectiles can break one.
pub fn blame(kind: super::MobKind) -> crate::simulation::damage::Cause {
    use super::MobKind as K;
    use crate::simulation::damage::{Cause, DamageType::PlayerExplosion};
    let text = match kind {
        K::Llama => "was blown up by a llama",
        K::TraderLlama => "was blown up by a trader llama",
        K::Skeleton => "was blown up by a skeleton",
        K::Stray => "was blown up by a stray",
        K::Bogged => "was blown up by a bogged",
        K::Parched => "was blown up by a parched",
        K::Pillager => "was blown up by a pillager",
        K::Piglin => "was blown up by a piglin",
        K::Illusioner => "was blown up by an illusioner",
        K::Drowned => "was blown up by a drowned",
        K::SnowGolem => "was blown up by a snow golem",
        K::Breeze => "was blown up by a breeze",
        K::Shulker => "was blown up by a shulker",
        K::Wither => "was blown up by the Wither",
        _ => return Cause::new(PlayerExplosion, "was blown up by a mob"),
    };
    Cause::new(PlayerExplosion, text)
}

/// A crystal that went this tick, for the fight (`onCrystalDestroyed`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Destroyed {
    pub id: u32,
    pub pos: DVec3,
    /// The player behind the blow, if one was.
    pub by: Option<PlayerId>,
}

/// Every End crystal in the dimension.
#[derive(Default)]
pub struct Crystals {
    pub list: Vec<Crystal>,
    next_id: u32,
    /// Crystals hit since the last tick, removed at the next one.
    pending: Vec<(u32, Blow)>,
    /// Crystals the last tick removed, for the fight.
    pub destroyed: Vec<Destroyed>,
}

impl Crystals {
    /// Adds a crystal standing at `pos` (feet); returns its id.
    pub fn spawn(&mut self, pos: DVec3, beam: Option<IVec3>, show_bottom: bool, invulnerable: bool) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        // Java starts `time` at `random.nextInt(100000)`; only the spin
        // and bob read it.
        let age = (id.wrapping_mul(2_654_435_761) >> 8) % 100_000;
        self.list.push(Crystal { id, pos, age, beam, show_bottom, invulnerable });
        id
    }

    pub fn get(&self, id: u32) -> Option<&Crystal> {
        self.list.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Crystal> {
        self.list.iter_mut().find(|c| c.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// The crystals whose boxes touch the box `min..max` (Java's
    /// `AABB.intersects`, strict on every side).
    pub fn touching(&self, min: DVec3, max: DVec3) -> impl Iterator<Item = &Crystal> {
        self.list.iter().filter(move |c| {
            let (lo, hi) = c.aabb();
            lo.cmplt(max).all() && hi.cmpgt(min).all()
        })
    }

    /// `EndCrystalItem.useOn` on the block `clicked`: only on obsidian or
    /// bedrock with an empty (air) cell above, and no entity in the 1x2x1
    /// box there (`occupied` answers for every other kind of entity).
    /// Returns where the new crystal stands.
    pub fn placement<W: BlockSource + ?Sized>(
        &self,
        world: &W,
        clicked: IVec3,
        occupied: impl Fn(DVec3, DVec3) -> bool,
    ) -> Option<DVec3> {
        if !matches!(world.block(clicked), Some(Block::OBSIDIAN | Block::BEDROCK)) {
            return None;
        }
        let above = clicked + IVec3::Y;
        if world.block(above) != Some(Block::AIR) {
            return None;
        }
        let (min, max) = (above.as_dvec3(), above.as_dvec3() + DVec3::new(1.0, 2.0, 1.0));
        // `Level.getEntities(null, box)` keeps entities whose boxes intersect.
        if self.touching(min, max).next().is_some() || occupied(min, max) {
            return None;
        }
        Some(above.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
    }

    /// Places a player's crystal (`setShowBottom(false)`).
    pub fn place(&mut self, pos: DVec3) -> u32 {
        self.spawn(pos, None, false, false)
    }

    /// `EndCrystal.hurtServer`: any blow but the dragon's destroys the
    /// crystal (at the next tick). Returns whether it was hurt.
    pub fn hit(&mut self, id: u32, blow: Blow) -> bool {
        let Some(c) = self.get(id) else { return false };
        // `isInvulnerableToBase`: invulnerable unless a creative player's.
        if c.invulnerable && !matches!(blow, Blow::Player { creative: true, .. }) {
            return false;
        }
        if !self.pending.iter().any(|&(i, _)| i == id) {
            self.pending.push((id, blow));
        }
        true
    }

    /// A blast of `power` at `center`: Java hurts every entity within
    /// twice the power of its position (shielded ones take at least one
    /// point), so every crystal in reach goes.
    pub fn explode(&mut self, center: DVec3, power: f32) {
        let reach = f64::from(power) * 2.0;
        let hit: Vec<u32> =
            self.list.iter().filter(|c| c.pos.distance_squared(center) <= reach * reach).map(|c| c.id).collect();
        for id in hit {
            self.hit(id, Blow::Explosion);
        }
    }

    /// Removes a crystal without a blast (Java's `discard`).
    pub fn discard(&mut self, id: u32) {
        self.list.retain(|c| c.id != id);
        self.pending.retain(|&(i, _)| i != id);
    }

    /// The nearest crystal a ray from `origin` along `dir` meets within
    /// `max_dist` (in units of `dir`).
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(u32, f64)> {
        self.list
            .iter()
            .filter_map(|c| {
                let (min, max) = c.aabb();
                physics::ray_aabb(origin, dir, min, max).filter(|&t| t <= max_dist).map(|t| (c.id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The crystal whose box holds `p`.
    pub fn hit_at(&self, p: DVec3) -> Option<u32> {
        self.list
            .iter()
            .find(|c| {
                let (min, max) = c.aabb();
                p.cmpge(min).all() && p.cmple(max).all()
            })
            .map(|c| c.id)
    }

    /// Crystals hit since the last tick go (blowing up unless a blast got
    /// them), each recorded in [`Crystals::destroyed`].
    pub(super) fn settle(&mut self, events: &mut Vec<EntityEvent>) {
        for (id, blow) in std::mem::take(&mut self.pending) {
            let Some(i) = self.list.iter().position(|c| c.id == id) else { continue };
            let c = self.list.remove(i);
            if blow != Blow::Explosion {
                // At its feet, named after the player (v1.0 gap Track 8c)
                // or mob behind the blow (`explosion(crystal,
                // source.getEntity())`, a `player_explosion`), or the
                // crystal's own with nobody behind it.
                let cause = match blow {
                    Blow::Mob(kind) => blame(kind),
                    Blow::Player { .. } => crate::simulation::damage::Cause::PLAYER_EXPLOSION,
                    _ => crate::simulation::damage::Cause::END_CRYSTAL,
                };
                events.push(EntityEvent::Explosion {
                    center: c.pos,
                    power: POWER,
                    cause,
                    credit_player: blow.player().is_some(),
                });
            }
            self.destroyed.push(Destroyed { id, pos: c.pos, by: blow.player() });
        }
    }

    /// Whether a hit waits for the next tick.
    pub fn hits_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// One tick: crystals hit since the last one go (blowing up unless a
    /// blast got them), the rest age, and in the End (`getDragonFight() !=
    /// null`) each keeps a fire in its own cell while that is air.
    pub fn tick<W: BlockSource + ?Sized>(&mut self, world: &W, in_end: bool, events: &mut Vec<EntityEvent>) {
        self.destroyed.clear();
        self.settle(events);
        for c in &mut self.list {
            c.age = c.age.wrapping_add(1);
            if in_end && world.block(c.cell()) == Some(Block::AIR) {
                events.push(EntityEvent::CrystalFire { cell: c.cell() });
            }
        }
    }

    /// The save: `x y z beam flags` per crystal, `;`-separated; the beam is
    /// `x,y,z` or `-`; flags are 1 for the bottom plate, 2 invulnerable.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for c in &self.list {
            if !out.is_empty() {
                out.push(';');
            }
            let beam = c.beam.map_or("-".to_string(), |b| format!("{},{},{}", b.x, b.y, b.z));
            let flags = u8::from(c.show_bottom) | u8::from(c.invulnerable) << 1;
            out += &format!("{} {} {} {beam} {flags}", c.pos.x, c.pos.y, c.pos.z);
        }
        out
    }

    /// Restores [`Crystals::serialize`]'s crystals, skipping broken entries.
    pub fn load(&mut self, text: &str) {
        for entry in text.split(';').filter(|s| !s.is_empty()) {
            let f: Vec<&str> = entry.split(' ').collect();
            let [x, y, z, beam, flags] = f[..] else { continue };
            let (Ok(x), Ok(y), Ok(z)) = (x.parse::<f64>(), y.parse::<f64>(), z.parse::<f64>()) else { continue };
            let pos = DVec3::new(x, y, z);
            if !pos.is_finite() {
                continue;
            }
            let beam = {
                let b: Vec<i32> = beam.split(',').filter_map(|v| v.parse().ok()).collect();
                match b[..] {
                    [x, y, z] => Some(IVec3::new(x, y, z)),
                    _ => None,
                }
            };
            let flags: u8 = flags.parse().unwrap_or(1);
            self.spawn(pos, beam, flags & 1 != 0, flags & 2 != 0);
        }
    }

    /// What a LAN client draws: position, age, beam and bottom plate.
    pub fn replica_json(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.list
                .iter()
                .take(64)
                .map(|c| {
                    serde_json::json!([
                        c.pos.x,
                        c.pos.y,
                        c.pos.z,
                        c.age,
                        c.beam.map(|b| [b.x, b.y, b.z]),
                        c.show_bottom,
                        c.id
                    ])
                })
                .collect(),
        )
    }

    /// A LAN client's copy of the host's [`Crystals::replica_json`].
    pub fn apply_replica(&mut self, v: &serde_json::Value) {
        self.list = v
            .as_array()
            .map(|cs| {
                cs.iter()
                    .take(64)
                    .filter_map(|c| {
                        let pos = DVec3::new(c.get(0)?.as_f64()?, c.get(1)?.as_f64()?, c.get(2)?.as_f64()?);
                        if !pos.is_finite() {
                            return None;
                        }
                        let beam = c.get(4).and_then(|b| {
                            let b = b.as_array()?;
                            Some(IVec3::new(
                                b.first()?.as_i64()? as i32,
                                b.get(1)?.as_i64()? as i32,
                                b.get(2)?.as_i64()? as i32,
                            ))
                        });
                        Some(Crystal {
                            id: c.get(6).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            pos,
                            age: c.get(3)?.as_u64()? as u32,
                            beam,
                            show_bottom: c.get(5).and_then(|v| v.as_bool()).unwrap_or(true),
                            invulnerable: false,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
}

/// The item's information.
pub fn item_info(item: Item) -> Option<crate::item::ItemInfo> {
    (item == END_CRYSTAL).then_some(crate::item::ItemInfo {
        name: "end crystal",
        kind: crate::item::ItemKind::Material,
        max_stack: 64,
        sprite: crate::item::Sprite::Gem([255, 140, 220]),
    })
}

/// Original 16×16 icon: a pink core in a pale glass cube, tilted.
pub fn icon_pixel(x: usize, y: usize) -> [u8; 4] {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
    // A diamond (the cube seen corner-on) with a frame and a glowing core.
    let d = dx.abs() + dy.abs();
    if d > 7.5 {
        return [0, 0, 0, 0];
    }
    if d > 6.0 {
        return [236, 226, 255, 255];
    }
    if d < 2.6 {
        let hot = d < 1.2;
        return if hot { [255, 214, 240, 255] } else { [236, 88, 186, 255] };
    }
    // Glass, with a lighter pane on the upper left.
    if dx + dy < -3.0 { [214, 206, 236, 200] } else { [170, 150, 200, 170] }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Cells(Vec<(IVec3, Block)>);
    impl BlockSource for Cells {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(self.0.iter().find(|(q, _)| *q == p).map_or(Block::AIR, |&(_, b)| b))
        }
    }

    #[test]
    fn crystals_go_only_on_obsidian_or_bedrock_with_room_above() {
        let world = Cells(vec![
            (IVec3::new(0, 64, 0), Block::OBSIDIAN),
            (IVec3::new(5, 64, 0), Block::BEDROCK),
            (IVec3::new(9, 64, 0), Block::STONE),
            (IVec3::new(12, 64, 0), Block::OBSIDIAN),
            (IVec3::new(12, 65, 0), Block::GLASS),
        ]);
        let mut crystals = Crystals::default();
        let none = |_: DVec3, _: DVec3| false;
        assert_eq!(crystals.placement(&world, IVec3::new(0, 64, 0), none), Some(DVec3::new(0.5, 65.0, 0.5)));
        assert!(crystals.placement(&world, IVec3::new(5, 64, 0), none).is_some());
        assert_eq!(crystals.placement(&world, IVec3::new(9, 64, 0), none), None, "stone");
        assert_eq!(crystals.placement(&world, IVec3::new(12, 64, 0), none), None, "a block above");
        // Another entity in the 1x2x1 box above blocks it, and so does a
        // crystal next door, whose 2x2x2 box reaches over.
        assert_eq!(crystals.placement(&world, IVec3::new(0, 64, 0), |_, _| true), None);
        crystals.place(DVec3::new(1.5, 65.0, 0.5));
        assert_eq!(crystals.placement(&world, IVec3::new(0, 64, 0), none), None);
        assert!(crystals.placement(&world, IVec3::new(5, 64, 0), none).is_some());
    }

    #[test]
    fn hits_blow_crystals_up_but_blasts_only_remove_them() {
        let mut crystals = Crystals::default();
        let a = crystals.place(DVec3::new(0.5, 65.0, 0.5));
        let b = crystals.place(DVec3::new(8.5, 65.0, 0.5));
        let far = crystals.place(DVec3::new(40.5, 65.0, 0.5));
        assert!(crystals.hit(a, Blow::Player { id: PlayerId::HOST, creative: false }));
        let mut events = Vec::new();
        crystals.tick(&Cells(Vec::new()), false, &mut events);
        assert_eq!(
            crystals.destroyed,
            vec![Destroyed { id: a, pos: DVec3::new(0.5, 65.0, 0.5), by: Some(PlayerId::HOST) }]
        );
        let blasts: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                EntityEvent::Explosion { center, power, credit_player, .. } => Some((*center, *power, *credit_player)),
                _ => None,
            })
            .collect();
        // At the crystal's feet, Java's power 6, credited to the player.
        assert_eq!(blasts, vec![(DVec3::new(0.5, 65.0, 0.5), POWER, true)]);
        // That blast reaches the crystal eight blocks off, which goes
        // without a blast of its own; the far one stays.
        crystals.explode(DVec3::new(0.5, 65.0, 0.5), POWER);
        events.clear();
        crystals.tick(&Cells(Vec::new()), false, &mut events);
        assert_eq!(crystals.destroyed.len(), 1);
        assert_eq!(crystals.destroyed[0].id, b);
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::Explosion { .. })));
        assert_eq!(crystals.list.len(), 1);
        assert_eq!(crystals.list[0].id, far);
    }

    #[test]
    fn invulnerable_crystals_take_only_creative_hits() {
        let mut crystals = Crystals::default();
        let id = crystals.spawn(DVec3::ZERO, Some(IVec3::new(0, 128, 0)), true, true);
        assert!(!crystals.hit(id, Blow::Player { id: PlayerId::HOST, creative: false }));
        assert!(!crystals.hit(id, Blow::Explosion));
        assert!(crystals.hit(id, Blow::Player { id: PlayerId::HOST, creative: true }));
    }

    #[test]
    fn crystals_light_their_cell_only_in_the_end_and_survive_a_save() {
        let mut crystals = Crystals::default();
        crystals.place(DVec3::new(0.5, 65.0, 0.5));
        crystals.spawn(DVec3::new(3.5, 80.0, -2.5), Some(IVec3::new(0, 128, 0)), true, true);
        let mut events = Vec::new();
        crystals.tick(&Cells(Vec::new()), false, &mut events);
        assert!(events.is_empty());
        crystals.tick(&Cells(Vec::new()), true, &mut events);
        assert!(events.contains(&EntityEvent::CrystalFire { cell: IVec3::new(0, 65, 0) }));
        let text = crystals.serialize();
        assert_eq!(text, "0.5 65 0.5 - 0;3.5 80 -2.5 0,128,0 3");
        let mut loaded = Crystals::default();
        loaded.load(&text);
        assert_eq!(loaded.list.len(), 2);
        assert!(!loaded.list[0].show_bottom && !loaded.list[0].invulnerable);
        assert_eq!(loaded.list[1].beam, Some(IVec3::new(0, 128, 0)));
        assert!(loaded.list[1].show_bottom && loaded.list[1].invulnerable);
        assert_eq!(loaded.serialize(), text);
        // A LAN client draws the same crystals.
        let mut client = Crystals::default();
        client.apply_replica(&crystals.replica_json());
        assert_eq!(client.list.len(), 2);
        assert_eq!(client.list[1].beam, Some(IVec3::new(0, 128, 0)));
    }
}
