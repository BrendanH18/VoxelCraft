//! Lingering potion components and Java AreaEffectCloud's bounded lifetime.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, PlayerId, Rng};
use crate::{inventory::Stack, item::Item, potion::Potion, simulation::effects::Effect};
use glam::DVec3;
use serde_json::{Value, json};

impl MobKind {
    pub fn accepts_potion_effect(self, e: Effect) -> bool {
        !(self.creature() == crate::enchant::Creature::Undead && matches!(e, Effect::Poison | Effect::Regeneration)
            || matches!(self, Self::Spider | Self::CaveSpider) && e == Effect::Poison
            || self == Self::WitherSkeleton && e == Effect::Wither)
    }
}
impl Stack {
    pub fn lingering_potion(p: Potion) -> Self {
        let mut s = Self::new(Item::LINGERING_POTION, 1);
        s.entity_data = u32::from(p.0) + 1;
        s
    }
    pub fn lingering_contents(self) -> Option<Potion> {
        (self.item == Item::LINGERING_POTION)
            .then_some(self.entity_data.saturating_sub(1))
            .filter(|i| *i < Potion::COUNT as u32)
            .map(|i| Potion(i as u8))
    }
}
#[derive(Clone, Debug)]
pub struct Cloud {
    pub pos: DVec3,
    pub potion: Potion,
    pub owner: Option<PlayerId>,
    pub radius: f32,
    pub age: u32,
    clock: f64,
    victims: Vec<(u64, u32)>,
}
impl Cloud {
    pub fn new(pos: DVec3, potion: Potion, owner: Option<PlayerId>) -> Self {
        Self { pos, potion, owner, radius: 3., age: 0, clock: 0., victims: Vec::new() }
    }
    fn contains(&self, pos: DVec3, min: DVec3, max: DVec3) -> bool {
        min.y < self.pos.y + 0.5
            && max.y > self.pos.y
            && pos.with_y(0.).distance_squared(self.pos.with_y(0.)) <= f64::from(self.radius).powi(2)
    }
    fn dose(&mut self, key: u64) -> bool {
        if self.victims.iter().any(|&(id, until)| id == key && until > self.age) {
            return false;
        }
        self.victims.push((key, self.age + 20));
        self.radius -= 0.5;
        true
    }
    pub(super) fn tick(
        &mut self,
        dt: f64,
        c: &Ctx,
        mobs: &mut [Mob],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.clock += dt;
        while self.clock >= 0.05 - 1e-9 {
            self.clock -= 0.05;
            self.age += 1;
            if self.age >= 610 || self.radius < 0.5 {
                return false;
            }
            if self.age < 10 {
                continue;
            }
            self.radius -= 3. / 600.;
            if self.radius < 0.5 {
                return false;
            }
            if !self.age.is_multiple_of(5) {
                continue;
            }
            self.victims.retain(|&(_, until)| until > self.age);
            if self.potion.effects().next().is_none() {
                self.victims.clear();
                continue;
            }
            for t in c.players.iter().filter(|t| t.alive && !t.spectator) {
                let (min, max) = t.shape.aabb(t.pos);
                if !self.contains(t.pos, min, max) || !self.dose(u64::from(t.id.0)) {
                    continue;
                }
                for (effect, amplifier, ticks) in self.potion.effects() {
                    if effect.is_instant() {
                        let base = if effect == Effect::InstantDamage { 3. } else { -2. };
                        events.push(EntityEvent::PlayerMagic {
                            player: t.id,
                            amount: base * (1u32 << amplifier.min(20)) as f32,
                        });
                    } else {
                        events.push(EntityEvent::PlayerEffect {
                            player: t.id,
                            effect,
                            amplifier,
                            ticks: (ticks / 4).max(1),
                        });
                    }
                }
                if self.radius < 0.5 {
                    return false;
                }
            }
            for m in mobs.iter_mut().filter(|m| m.alive()) {
                if !self.potion.effects().any(|(e, _, _)| m.kind.accepts_potion_effect(e)) {
                    continue;
                }
                let (min, max) = m.aabb();
                if !self.contains(m.pos, min, max) || !self.dose((1u64 << 32) | u64::from(m.uid)) {
                    continue;
                }
                let undead = m.kind.creature() == crate::enchant::Creature::Undead;
                for (effect, amp, ticks) in self.potion.effects() {
                    if !m.kind.accepts_potion_effect(effect) {
                        continue;
                    }
                    if !effect.is_instant() {
                        m.effects.add(effect, amp, (ticks / 4).max(1));
                        if effect == Effect::Weakness {
                            m.weakness_left = m.weakness_left.max((ticks / 4) as f32 / 20.);
                        }
                        continue;
                    }
                    let harms = (effect == Effect::InstantDamage) != undead;
                    let amount = (if harms { 3. } else { 2. }) * (1u32 << amp.min(20)) as f32;
                    if harms {
                        if self.owner.is_some() {
                            m.player_hit();
                        }
                        let killed = m.damage(if m.kind == MobKind::Witch { amount * 0.15 } else { amount }, None, rng);
                        events.push(EntityEvent::MobShot {
                            kind: m.kind,
                            pos: m.pos,
                            killed,
                            burning: m.burning,
                            player_kill: self.owner.is_some(),
                            owner: self.owner,
                        });
                    } else {
                        m.health = (m.health + amount).min(m.max_health());
                    }
                }
                if self.radius < 0.5 {
                    return false;
                }
            }
        }
        true
    }
    pub fn save(&self) -> Value {
        json!({"pos":self.pos.to_array(),"potion":self.potion.0,"owner":self.owner.map(|p|p.0),"radius":self.radius,"age":self.age,"clock":self.clock,"victims":self.victims})
    }
    pub fn load(v: &Value) -> Option<Self> {
        let pos = v["pos"].as_array().filter(|a| a.len() == 3)?;
        let pos = DVec3::new(pos[0].as_f64()?, pos[1].as_f64()?, pos[2].as_f64()?);
        if !pos.is_finite() {
            return None;
        }
        let p = v["potion"].as_u64().filter(|i| *i < Potion::COUNT as u64)?;
        let mut s =
            Self::new(pos, Potion(p as u8), v["owner"].as_u64().and_then(|i| u32::try_from(i).ok()).map(PlayerId));
        s.radius = v["radius"].as_f64().filter(|x| x.is_finite() && (0.5..=3.).contains(x))? as f32;
        s.age = v["age"].as_u64()?.min(610) as u32;
        s.clock = v["clock"].as_f64().filter(|x| x.is_finite() && (0.0..0.05).contains(x)).unwrap_or(0.);
        if let Some(a) = v["victims"].as_array() {
            s.victims = a
                .iter()
                .take(4096)
                .filter_map(|p| Some((p[0].as_u64()?, p[1].as_u64()?.min(u64::from(s.age + 20)) as u32)))
                .collect();
        }
        Some(s)
    }
}
impl Entities {
    pub fn collect_dragon_breath(&mut self, pos: DVec3, shape: crate::physics::Shape) -> bool {
        let Some(f) = &mut self.fight else {
            return false;
        };
        let (min, max) = shape.aabb(pos);
        let min = min - DVec3::splat(2.);
        let max = max + DVec3::splat(2.);
        let Some(c) = f.clouds.iter_mut().find(|c| {
            c.pos.y < max.y
                && c.pos.y + 0.5 > min.y
                && c.pos.x - f64::from(c.radius) < max.x
                && c.pos.x + f64::from(c.radius) > min.x
                && c.pos.z - f64::from(c.radius) < max.z
                && c.pos.z + f64::from(c.radius) > min.z
        }) else {
            return false;
        };
        c.radius = (c.radius - 0.5).max(0.);
        true
    }
    pub(super) fn cloud_json(&self) -> String {
        let v: Vec<_> = self
            .clouds
            .iter()
            .chain(self.storage.chunks().into_iter().flat_map(|c| &c.clouds))
            .map(Cloud::save)
            .collect();
        serde_json::to_string(&v).unwrap()
    }
    pub(super) fn load_cloud_json(&mut self, text: &str) {
        if let Ok(Value::Array(a)) = serde_json::from_str(text) {
            self.clouds = a.iter().take(4096).filter_map(Cloud::load).collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crafting::Grid,
        entity::Target,
        world::{
            brewing::{BrewingStand, brew_stack},
            terrain::Dimension,
        },
    };
    fn context() -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId(7), DVec3::ZERO, true)],
            spawning: false,
            daylight: 0.,
            raining: false,
            dimension: Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        }
    }
    #[test]
    fn lingering_brewing_and_eight_arrow_crafting_preserve_every_registered_potion() {
        for p in Potion::all() {
            let s = brew_stack(Stack::new(Item::splash_potion(p), 1), Item::DRAGON_BREATH).unwrap();
            assert_eq!(s.lingering_contents(), Some(p));
            let saved = crate::inventory::stack_to_string(Some(s));
            assert_eq!(crate::inventory::stack_from_str(&saved), Some(Some(s)));
            let mut g = Grid::new(3);
            g.cells = [Some(Stack::new(Item::ARROW, 1)); 9];
            g.cells[4] = Some(s);
            assert_eq!(g.result(), Some(Stack::tipped_arrow(p, 8)));
            g.consume();
            assert!(g.cells.iter().all(Option::is_none));
        }
        let awkward = Potion::from_id("awkward").unwrap();
        assert_eq!(
            brew_stack(Stack::new(Item::potion(awkward), 1), Item::PHANTOM_MEMBRANE).unwrap().item.as_potion(),
            Potion::from_id("slow_falling")
        );
    }
    #[test]
    fn breath_brewing_returns_a_bottle_and_saves_in_progress_contents() {
        let mut s = BrewingStand::default();
        s.bottles[0] = Some(Stack::new(Item::splash_potion(Potion::from_id("poison").unwrap()), 1));
        s.ingredient = Some(Stack::new(Item::DRAGON_BREATH, 1));
        s.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
        assert!(!s.tick(0.05));
        s = BrewingStand::deserialize(&s.serialize()).unwrap();
        for _ in 0..400 {
            s.tick(0.05);
        }
        assert_eq!(s.ingredient.unwrap().item, Item::GLASS_BOTTLE);
        assert_eq!(s.bottles[0].unwrap().lingering_contents(), Potion::from_id("poison"));
    }
    #[test]
    fn cloud_wait_radius_duration_cooldown_and_reload_match_reference() {
        let p = Potion::from_id("poison").unwrap();
        let mut c = Cloud::new(DVec3::ZERO, p, Some(PlayerId(7)));
        let mut ev = Vec::new();
        let ctx = context();
        let mut rng = Rng::new(1);
        for _ in 0..9 {
            assert!(c.tick(0.05, &ctx, &mut [], &mut rng, &mut ev));
        }
        assert!(ev.is_empty());
        assert!(c.tick(0.05, &ctx, &mut [], &mut rng, &mut ev));
        assert!(ev.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { ticks: 225, .. })));
        assert!((c.radius - 2.495).abs() < 1e-5);
        c = Cloud::load(&c.save()).unwrap();
        ev.clear();
        for _ in 0..19 {
            c.tick(0.05, &ctx, &mut [], &mut rng, &mut ev);
        }
        assert!(ev.is_empty());
        c.tick(0.05, &ctx, &mut [], &mut rng, &mut ev);
        assert!(!ev.is_empty());
        let mut e = Entities::new(3);
        e.clouds.push(c);
        let mut props = Default::default();
        e.save_props(&mut props);
        let mut copy = Entities::new(4);
        copy.load_props(&props);
        assert_eq!(copy.clouds[0].potion, p);
        assert_eq!(copy.clouds[0].age, e.clouds[0].age);
    }
    #[test]
    fn existing_dragon_clouds_are_a_survival_breath_source() {
        let mut e = Entities::new(5);
        let mut fight = super::super::dragon::Fight::new(&crate::world::end::EndGen::new(7));
        fight.clouds.push(super::super::dragon::BreathCloud::new(DVec3::ZERO, 3., 0., 200, 3.));
        e.fight = Some(fight);
        assert!(!e.collect_dragon_breath(DVec3::X * 20., crate::player::SHAPE));
        assert!(e.collect_dragon_breath(DVec3::ZERO, crate::player::SHAPE));
        assert_eq!(e.fight.as_ref().unwrap().clouds[0].radius, 2.5);
    }
    #[test]
    fn immune_mobs_do_not_use_up_a_poison_cloud() {
        let mut c = Cloud::new(DVec3::ZERO, Potion::from_id("poison").unwrap(), None);
        let mut ctx = context();
        ctx.players.clear();
        let mut mobs = [Mob::new(MobKind::Stray, DVec3::ZERO, 0.), Mob::new(MobKind::Spider, DVec3::ZERO, 0.)];
        for _ in 0..10 {
            c.tick(0.05, &ctx, &mut mobs, &mut Rng::new(4), &mut Vec::new());
        }
        assert!((c.radius - 2.995).abs() < 1e-5);
        assert!(mobs.iter().all(|m| !m.effects.has(Effect::Poison)));
    }

    #[test]
    fn brewing_preserves_custom_components_while_changing_potion_contents() {
        let p = Potion::from_id("poison").unwrap();
        let s = Stack::new(Item::splash_potion(p), 1).with_name("Night mist").unwrap();
        let lingering = brew_stack(s, Item::DRAGON_BREATH).unwrap();
        let long = brew_stack(lingering, Item::REDSTONE).unwrap();
        assert_eq!(long.name.as_str(), Some("Night mist"));
        assert_eq!(long.lingering_contents(), Potion::from_id("long_poison"));
    }
}
