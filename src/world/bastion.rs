//! Bastion-remnant loot data reserved for the later bastion generator.
//! Netherite upgrade templates are not injected into unrelated chests.

use crate::item::Item;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestKind {
    Treasure,
    Bridge,
    HoglinStable,
    Other,
}

/// A rational independent chance for one item in a bastion chest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LootChance {
    pub item: Item,
    pub count: u8,
    pub numerator: u8,
    pub denominator: u8,
}

/// Java 1.21's template pool: every treasure-room chest has one; bridge,
/// hoglin-stable and generic bastion chests have a one-in-ten chance.
pub const NETHERITE_UPGRADE: [(ChestKind, LootChance); 4] = [
    (ChestKind::Treasure, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 1 }),
    (ChestKind::Bridge, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
    (ChestKind::HoglinStable, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
    (ChestKind::Other, LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }),
];

pub fn netherite_upgrade(kind: ChestKind) -> LootChance {
    NETHERITE_UPGRADE.iter().find(|(chest, _)| *chest == kind).expect("all bastion chest kinds").1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_template_matches_java_bastion_tables() {
        for kind in [ChestKind::Bridge, ChestKind::HoglinStable, ChestKind::Other] {
            assert_eq!(
                netherite_upgrade(kind),
                LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 10 }
            );
        }
        assert_eq!(
            netherite_upgrade(ChestKind::Treasure),
            LootChance { item: Item::NETHERITE_UPGRADE, count: 1, numerator: 1, denominator: 1 }
        );
    }
}
