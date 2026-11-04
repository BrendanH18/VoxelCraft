//! Potion components of tipped arrows and their pickup stacks. Potion indices
//! use Stack's existing saved component payload; old ordinary arrows stay zero.
use super::{Arrow, Entities, EntityEvent, Mob, PlayerId, Rng};
use crate::{inventory::Stack, item::Item, potion::Potion, simulation::effects::Effect};
use glam::DVec3;

/// Adapter transaction state for physical arrow damage and its potion effects.
/// Feed actual damage accepted by the player, then the matching event markers.
pub struct ArrowEffectGate { last:Option<(PlayerId,bool)>, allowed:bool }
impl Default for ArrowEffectGate { fn default()->Self {Self{last:None,allowed:true}} }
impl ArrowEffectGate {
    pub fn record_hit(&mut self,player:PlayerId,accepted:f32){self.last=Some((player,accepted>0.));}
    pub fn begin(&mut self,player:PlayerId){self.allowed=self.last==Some((player,true));}
    pub fn end(&mut self){self.allowed=true;self.last=None;}
    pub fn allows(&self)->bool{self.allowed}
}

impl Stack {
    pub fn tipped_arrow(potion: Potion, count: u8) -> Self {
        let mut stack = Self::new(Item::TIPPED_ARROW, count);
        stack.entity_data = u32::from(potion.0) + 1;
        stack
    }
    /// Potion component on a tipped arrow or the bolt held in a charged crossbow.
    pub fn arrow_potion(self) -> Option<Potion> {
        if !matches!(self.item, Item::TIPPED_ARROW | Item::CHARGED_CROSSBOW) {
            return None;
        }
        self.entity_data.checked_sub(1).filter(|&i| i < Potion::COUNT as u32).map(|i| Potion(i as u8))
    }
}
impl Arrow {
    pub fn pickup_stack(&self) -> Stack {
        self.potion.map_or_else(
            || Stack::new(if self.tipped { Item::TIPPED_ARROW } else { Item::ARROW }, 1),
            |p| Stack::tipped_arrow(p, 1),
        )
    }
    fn effects(&self) -> impl Iterator<Item = (Effect, u8, u32)> + '_ {
        self.potion
            .into_iter()
            .flat_map(|p| p.effects())
            .map(|(effect, amp, ticks)| (effect, amp, (ticks / 8).max(1)))
            .chain(self.custom_effect)
    }
    pub(super) fn effects_on_player(&self, player: PlayerId, events: &mut Vec<EntityEvent>) {
        for (effect, amplifier, ticks) in self.effects() {
            if effect.is_instant() {
                let base = if effect == Effect::InstantDamage { 6.0 } else { -4.0 };
                events.push(EntityEvent::PlayerMagic { player, amount: base * (1u32 << amplifier.min(20)) as f32 });
            } else {
                events.push(EntityEvent::PlayerEffect { player, effect, amplifier, ticks });
            }
        }
    }
    pub(super) fn effects_on_mob(&self, mob: &mut Mob, rng: &mut Rng) {
        for (effect, amplifier, ticks) in self.effects() {
            let undead = mob.kind.creature() == crate::enchant::Creature::Undead;
            if !mob.kind.accepts_potion_effect(effect) {
                continue;
            }
            if !effect.is_instant() {
                mob.effects.add(effect, amplifier, ticks);
                if effect == Effect::Weakness {
                    mob.weakness_left = mob.weakness_left.max(ticks as f32 / 20.0);
                }
                continue;
            }
            let harms = (effect == Effect::InstantDamage) != undead;
            let base = if harms { 6.0 } else { 4.0 };
            let amount = base * (1u32 << amplifier.min(20)) as f32;
            if harms {
                let amount = if mob.kind == super::MobKind::Witch { amount * 0.15 } else { amount };
                mob.damage(amount, None, rng);
            } else {
                mob.health = (mob.health + amount).min(mob.max_health());
            }
        }
    }
}
impl Entities {
    /// Shared host, controller and CLI pickup retaining potion components.
    pub fn collect_arrow_stacks(&mut self, feet: DVec3) -> Vec<Stack> {
        let mut stacks: Vec<Stack> = Vec::new();
        self.arrows.retain(|a| {
            let near = (a.pos - (feet + DVec3::Y * 0.9)).abs().cmple(DVec3::new(1.3, 1.5, 1.3)).all();
            if !a.pickup || !a.is_stuck() || !near {
                return true;
            }
            let stack = a.pickup_stack();
            if let Some(s) = stacks.iter_mut().find(|s| s.stacks_with(&stack) && s.count < s.max()) {
                s.count += 1;
            } else {
                stacks.push(stack);
            }
            false
        });
        stacks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Ctx, MobKind, Target},
        physics::test_util::Grid,
    };
    fn ctx() -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(3., 10., 0.5), true)],
            spawning: false,
            daylight: 0.,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        }
    }
    #[test]
    fn potion_components_round_trip_without_merging_different_potions() {
        for p in Potion::all() {
            let s = Stack::tipped_arrow(p, 8);
            let text = crate::inventory::stack_to_string(Some(s));
            assert_eq!(crate::inventory::stack_from_str(&text), Some(Some(s)));
            assert_eq!(s.arrow_potion(), Some(p));
        }
        assert!(!Stack::tipped_arrow(Potion(11), 1).stacks_with(&Stack::tipped_arrow(Potion(20), 1)));
    }
    #[test]
    fn mob_custom_effects_and_loot_potion_arrows_have_distinct_durations() {
        for (kind, potion, ticks) in [(MobKind::Stray, "slowness", 600), (MobKind::Bogged, "poison", 100)] {
            let mut arrow = Arrow::aimed(DVec3::new(0., 11., 0.5), DVec3::new(3., 11., 0.5), "test", &mut Rng::new(8));
            arrow.custom_effect = kind.arrow_effect();
            assert!(!arrow.pickup);
            let mut events = Vec::new();
            for _ in 0..40 {
                if !arrow.update(0.05, &Grid::flat(10), &ctx(), &mut [], &[], None, &mut Rng::new(1), &mut events) {
                    break;
                }
            }
            assert!(
                events.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { ticks:t, .. } if *t==ticks)),
                "{events:?}"
            );
            let p = Potion::from_id(potion).unwrap();
            let mut player_arrow = Arrow::shot(DVec3::ZERO, DVec3::X, 1., true);
            player_arrow.potion = Some(p);
            assert_eq!(player_arrow.effects().next().unwrap().2, p.effects().next().unwrap().2 / 8);
        }
    }
    #[test]
    fn embedded_tipped_arrows_keep_their_payload_on_pickup() {
        let world = Grid::flat(10);
        let p = Potion::from_id("poison").unwrap();
        let mut a = Arrow::shot(DVec3::new(0.5, 12., 0.5), DVec3::NEG_Y, 1., true);
        a.potion = Some(p);
        a.tipped = true;
        for _ in 0..30 {
            if a.is_stuck() {
                break;
            }
            a.update(0.05, &world, &ctx(), &mut [], &[], None, &mut Rng::new(9), &mut Vec::new());
        }
        assert!(a.is_stuck());
        let mut e = Entities::new(7);
        let mut unpickupable = Arrow::aimed(a.pos, a.pos + DVec3::X, "mob", &mut Rng::new(8));
        unpickupable.custom_effect = MobKind::Bogged.arrow_effect();
        e.arrows.push(a);
        e.arrows.push(unpickupable);
        let stacks = e.collect_arrow_stacks(DVec3::new(0.5, 10., 0.5));
        assert_eq!(stacks, vec![Stack::tipped_arrow(p, 1)]);
        assert_eq!(e.arrows.len(), 1);
    }

    #[test]
    fn poison_immunity_and_inverted_instant_effects_apply_to_undead() {
        let mut a = Arrow::shot(DVec3::ZERO, DVec3::X, 1., true);
        let mut m = Mob::new(MobKind::Stray, DVec3::ZERO, 0.);
        a.potion = Potion::from_id("poison");
        a.effects_on_mob(&mut m, &mut Rng::new(1));
        assert!(!m.effects.has(Effect::Poison));
        m.health = 10.;
        a.potion = Potion::from_id("harming");
        a.effects_on_mob(&mut m, &mut Rng::new(1));
        assert_eq!(m.health, 14.);
        m.health = 10.;
        a.potion = Potion::from_id("healing");
        a.effects_on_mob(&mut m, &mut Rng::new(1));
        assert_eq!(m.health, 4.);
    }
    #[test]
    fn a_shield_blocks_the_tipped_effect_with_the_arrow(){
        for shield in [false,true] {
            let mut victim=crate::agent::Agent::new(DVec3::ZERO);victim.session.id=PlayerId(7);
            if shield {
                victim.session.inventory.offhand=Some(Stack::new(Item::SHIELD,1));
                assert!(victim.session.start_blocking(0));for _ in 0..5{victim.session.tick_blocking(0,true);}
            }
            let mut arrow=Arrow::aimed(DVec3::new(3.,1.2,0.),DVec3::new(0.,1.2,0.),"was shot by a bogged",&mut Rng::new(9));
            arrow.custom_effect=MobKind::Bogged.arrow_effect();
            let mut c=ctx();c.players=vec![Target::new(PlayerId(7),DVec3::ZERO,true)];
            let mut events=Vec::new();for _ in 0..40 {if !arrow.update(0.05,&Grid::flat(0),&c,&mut [],&[],None,&mut Rng::new(7),&mut events){break;}}
            let mut gate=ArrowEffectGate::default();let mut entities=Entities::new(1);
            for event in events {match event {
                EntityEvent::PlayerHit{player,damage,java:Some(hit),cause,..}=>gate.record_hit(player,victim.hurt_knocked(damage,cause,&hit,&mut entities)),
                EntityEvent::ArrowEffectsStart{player}=>gate.begin(player),
                EntityEvent::ArrowEffectsEnd=>gate.end(),
                EntityEvent::PlayerEffect{effect,amplifier,ticks,..} if gate.allows()=>{victim.session.vitals.apply_effect(effect,amplifier,ticks);},
                _=>{}
            }}
            assert_eq!(victim.session.vitals.effects.has(Effect::Poison),!shield);
            if shield {assert_eq!(victim.session.vitals.health,20.);assert!(victim.session.inventory.offhand.unwrap().damage>0);}else{assert!(victim.session.vitals.health<20.);}
        }
    }

}
