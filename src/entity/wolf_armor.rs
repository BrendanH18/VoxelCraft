//! Java 26.3 wolf body armor: owner actions, whole-hit absorption and dyed RGB.
use super::{Entities, EntityEvent, Mob, MobKind, MobSound, PlayerId, Rng};
use crate::{
    color::DyeColor,
    enchant::Enchantment,
    inventory::{Stack, components, stack_from_str},
    item::Item,
    world::block::Block,
};
use glam::DVec3;
pub const DURABILITY: u16 = 64;
pub use crate::inventory::components::DYED;
#[derive(Clone, Debug, Default)]
pub struct State {
    pub armor: Option<Stack>,
    fall_distance: f64,
    pub(super) pending_fall: f32,
}
impl State {
    pub(super) fn load(&mut self, text: Option<&str>) {
        self.armor = text
            .and_then(|s| stack_from_str(s).flatten())
            .filter(|s| s.item == Item::WOLF_ARMOR && s.damage < DURABILITY)
            .map(|s| Stack { count: 1, ..s });
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Equip,
    Repair,
    Remove,
}
/// Wolf armor's `dyed_color` (the shared [`components::dyed_color`]).
pub fn color(stack: Stack) -> Option<[u8; 3]> {
    if stack.item == Item::WOLF_ARMOR { components::dyed_color(&stack) } else { None }
}
pub fn item_tint_bytes(stack: Stack) -> [u8; 4] {
    color(stack).map_or([255; 4], |c| [c[0], c[1], c[2], 255])
}
pub fn item_tint(stack: Stack) -> [f32; 4] {
    color(stack).map_or([1.0; 4], |c| [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0, 1.0])
}
/// DyedItemColor's integer channel averages and float brightness
/// normalization ([`components::apply_dyes`]), for wolf armor.
pub fn dye(stack: Stack, dyes: &[DyeColor]) -> Option<Stack> {
    if stack.item != Item::WOLF_ARMOR {
        return None;
    }
    components::apply_dyes(stack, dyes)
}
pub fn washed(stack: Stack) -> Option<Stack> {
    color(stack)?;
    components::remove_dye(stack)
}
pub fn cracks(damage: u16) -> u8 {
    let remaining = 1.0 - f32::from(damage) / f32::from(DURABILITY);
    if remaining < 0.32 {
        3
    } else if remaining < 0.69 {
        2
    } else if remaining < 0.95 {
        1
    } else {
        0
    }
}
impl Mob {
    pub(super) fn absorb_wolf_armor(&mut self, amount: f32, rng: &mut Rng) -> f32 {
        let Some(s) = &mut self.wolf_armor else { return amount };
        let Some(a) = &mut s.armor else { return amount };
        if amount <= 0.0 {
            return amount;
        }
        let uses = amount.ceil().min(u16::MAX as f32) as u16;
        if !a.enchants.has(Enchantment::Unbreaking) {
            a.damage = a.damage.saturating_add(uses);
        } else {
            for _ in 0..uses {
                if crate::enchant::wears(a.enchants, false, rng.next_f32()) {
                    a.damage = a.damage.saturating_add(1);
                }
                if a.damage >= DURABILITY {
                    break;
                }
            }
        }
        if a.damage >= DURABILITY {
            s.armor = None;
        }
        // The hit that breaks the armor is absorbed in full.
        0.0
    }
    pub(super) fn wolf_fall(&mut self, from: f64, landed: bool, below: Option<Block>) {
        let Some(s) = &mut self.wolf_armor else { return };
        if self.in_water {
            s.fall_distance = 0.0;
        } else {
            s.fall_distance += (from - self.pos.y).max(0.0);
        }
        if landed {
            let distance = std::mem::take(&mut s.fall_distance);
            if below != Some(crate::world::stone_blocks::SLIME_BLOCK) {
                s.pending_fall += (distance - 3.0 - f64::from(self.effects.jump_boost())).ceil().max(0.0) as f32;
            }
        }
    }
}
impl Entities {
    pub fn use_wolf_armor(
        &mut self,
        index: usize,
        held: Option<Stack>,
        player: PlayerId,
        creative: bool,
    ) -> Option<Use> {
        let hand = held?;
        let m = self.mobs.get_mut(index).filter(|m| {
            m.kind == MobKind::Wolf && m.alive() && m.animal.as_ref().is_some_and(|a| a.owner == Some(player))
        })?;
        let s = m.wolf_armor.as_mut()?;
        let result = if hand.item == Item::WOLF_ARMOR
            && s.armor.is_none()
            && !m.baby
            && m.age >= 0
            && hand.damage < DURABILITY
        {
            s.armor = Some(Stack { count: 1, ..hand });
            Use::Equip
        } else if hand.item == Item::ARMADILLO_SCUTE
            && m.animal.as_ref().unwrap().sitting
            && let Some(a) = s.armor.as_mut().filter(|a| a.damage > 0)
        {
            a.damage = a.damage.saturating_sub(DURABILITY / 8);
            Use::Repair
        } else if hand.item == Item::SHEARS
            && let Some(a) = s.armor.filter(|a| creative || !a.enchants.has(Enchantment::BindingCurse))
        {
            s.armor = None;
            let pos = m.pos;
            self.items.push(super::item::ItemEntity::new(a, pos, DVec3::ZERO, 0.5, &mut self.rng));
            Use::Remove
        } else {
            return None;
        };
        let pos = self.mobs[index].pos;
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Wolf), pos });
        Some(result)
    }
}
#[cfg(test)]
mod tests;
