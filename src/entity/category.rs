//! Java 26.3 `MobCategory`: natural spawn caps and despawn distances, and the
//! spawnable-chunk bookkeeping `NaturalSpawner.SpawnState` and
//! `LocalMobCapCalculator` keep for them.
//!
//! Sources: `net.minecraft.world.entity.MobCategory`, `EntityTypes` (each
//! type's category), `server.level.DistanceManager` (natural spawn chunks are
//! those within chessboard distance 8 of a player's chunk) and
//! `server.level.ChunkMap.playerIsCloseEnoughForSpawning` (a chunk's centre
//! within 128 blocks horizontally).
use std::collections::{HashMap, HashSet};

use glam::{DVec3, IVec2};

use super::{Mob, MobKind};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MobCategory {
    Monster,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}

impl MobCategory {
    pub const SPAWNING: [MobCategory; 7] = [
        MobCategory::Monster,
        MobCategory::Creature,
        MobCategory::Ambient,
        MobCategory::Axolotls,
        MobCategory::UndergroundWaterCreature,
        MobCategory::WaterCreature,
        MobCategory::WaterAmbient,
    ];

    /// `getMaxInstancesPerChunk`: the cap per 289 spawnable chunks, and each
    /// player's local cap.
    pub const fn max(self) -> u32 {
        match self {
            MobCategory::Monster => 70,
            MobCategory::Creature => 10,
            MobCategory::Ambient => 15,
            MobCategory::Axolotls | MobCategory::UndergroundWaterCreature | MobCategory::WaterCreature => 5,
            MobCategory::WaterAmbient => 20,
            MobCategory::Misc => 0,
        }
    }

    /// `getDespawnDistance`: beyond this a despawnable mob is removed at once.
    pub const fn despawn_distance(self) -> f64 {
        match self {
            MobCategory::WaterAmbient => 64.0,
            _ => 128.0,
        }
    }

    /// `getNoDespawnDistance`: within this nothing despawns at random.
    pub const NO_DESPAWN_DISTANCE: f64 = 32.0;

    /// `isFriendly`: still spawns with `spawnEnemies` off (Peaceful).
    pub const fn friendly(self) -> bool {
        !matches!(self, MobCategory::Monster)
    }

    /// `isPersistent`: spawns only on every 400th tick.
    pub const fn persistent(self) -> bool {
        matches!(self, MobCategory::Creature | MobCategory::Misc)
    }

    fn index(self) -> usize {
        self as usize
    }
}

impl MobKind {
    /// The entity type's Java category.
    pub const fn category(self) -> MobCategory {
        use MobKind as K;
        match self {
            K::Pig
            | K::Cow
            | K::Sheep
            | K::Chicken
            | K::WanderingTrader
            | K::TraderLlama
            | K::Strider
            | K::Cat
            | K::Turtle
            | K::Wolf
            | K::Fox
            | K::Parrot
            | K::Rabbit
            | K::Goat
            | K::Horse
            | K::Donkey
            | K::Mule
            | K::Llama
            | K::Camel => MobCategory::Creature,
            K::Villager | K::IronGolem | K::SnowGolem => MobCategory::Misc,
            K::Cod | K::Salmon | K::TropicalFish | K::Pufferfish => MobCategory::WaterAmbient,
            K::Squid | K::Dolphin => MobCategory::WaterCreature,
            K::GlowSquid => MobCategory::UndergroundWaterCreature,
            K::Axolotl => MobCategory::Axolotls,
            _ => MobCategory::Monster,
        }
    }
}

/// A 16-block chunk column, as Java's `ChunkPos`.
pub type ChunkPos = IVec2;

pub fn chunk_of(p: DVec3) -> ChunkPos {
    IVec2::new((p.x.floor() as i32).div_euclid(16), (p.z.floor() as i32).div_euclid(16))
}

/// `playerIsCloseEnoughForSpawning`: the chunk's centre within 128 blocks.
pub fn close_for_spawning(chunk: ChunkPos, player: DVec3) -> bool {
    let (x, z) = (chunk.x as f64 * 16.0 + 8.0, chunk.y as f64 * 16.0 + 8.0);
    (x - player.x).powi(2) + (z - player.z).powi(2) < 16384.0
}

/// One spawn round's `SpawnState`: spawnable chunks, category counts and
/// per-player local counts.
pub struct SpawnState {
    /// `getNaturalSpawnChunkCount`: chunks within chessboard distance 8 of any player.
    pub chunk_count: usize,
    /// Candidate chunks close enough to a player to spawn in, in a stable order.
    pub chunks: Vec<ChunkPos>,
    players: Vec<DVec3>,
    counts: [u32; 8],
    /// Per player, per category.
    local: Vec<[u32; 8]>,
}

impl SpawnState {
    /// `NaturalSpawner.createState` over `mobs` for these players. Mobs that
    /// require persistence don't count, as in Java.
    pub fn new<'a>(
        players: &[DVec3],
        mobs: impl IntoIterator<Item = &'a Mob>,
        loaded: impl Fn(ChunkPos) -> bool,
    ) -> Self {
        let mut near: HashSet<ChunkPos> = HashSet::new();
        for &p in players {
            let c = chunk_of(p);
            for dz in -8..=8 {
                for dx in -8..=8 {
                    near.insert(c + IVec2::new(dx, dz));
                }
            }
        }
        let mut chunks: Vec<ChunkPos> =
            near.iter().copied().filter(|&c| loaded(c) && players.iter().any(|&p| close_for_spawning(c, p))).collect();
        chunks.sort_unstable_by_key(|c| (c.y, c.x));
        let mut state = Self {
            chunk_count: near.len(),
            chunks,
            players: players.to_vec(),
            counts: [0; 8],
            local: vec![[0; 8]; players.len()],
        };
        let mut seen: HashMap<ChunkPos, bool> = HashMap::new();
        for m in mobs {
            if !m.alive() || m.persistent {
                continue;
            }
            let c = chunk_of(m.pos);
            if !*seen.entry(c).or_insert_with(|| loaded(c)) {
                continue;
            }
            state.add(m.kind.category(), m.pos);
        }
        state
    }

    /// `afterSpawn`/`createState` bookkeeping for one mob at `pos`.
    pub fn add(&mut self, category: MobCategory, pos: DVec3) {
        if category == MobCategory::Misc {
            return;
        }
        self.counts[category.index()] += 1;
        let c = chunk_of(pos);
        for (i, &p) in self.players.iter().enumerate() {
            if close_for_spawning(c, p) {
                self.local[i][category.index()] += 1;
            }
        }
    }

    pub fn count(&self, category: MobCategory) -> u32 {
        self.counts[category.index()]
    }

    /// `canSpawnForCategoryGlobal`: under max x spawnable chunks / 17².
    pub fn global_room(&self, category: MobCategory) -> bool {
        (self.counts[category.index()] as usize) < category.max() as usize * self.chunk_count / 289
    }

    /// `LocalMobCapCalculator.canSpawn`: some player near the chunk is under
    /// its own cap.
    pub fn local_room(&self, category: MobCategory, chunk: ChunkPos) -> bool {
        self.players
            .iter()
            .zip(&self.local)
            .any(|(&p, counts)| close_for_spawning(chunk, p) && counts[category.index()] < category.max())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_player_has_289_counted_chunks_and_a_128_block_spawn_disc() {
        let state = SpawnState::new(&[DVec3::new(8.0, 64.0, 8.0)], std::iter::empty(), |_| true);
        assert_eq!(state.chunk_count, 289);
        // Chunk centres strictly within 128 blocks of the centre of chunk (0, 0).
        let disc = (-8..=8)
            .flat_map(|x| (-8..=8).map(move |z| (x, z)))
            .filter(|&(x, z): &(i32, i32)| ((x * 16) as f64).powi(2) + ((z * 16) as f64).powi(2) < 16384.0)
            .count();
        assert_eq!(state.chunks.len(), disc);
        assert!(state.global_room(MobCategory::Monster));
        assert_eq!(MobCategory::Monster.max() as usize * state.chunk_count / 289, 70);
    }

    #[test]
    fn caps_count_non_persistent_mobs_globally_and_per_player() {
        let players = [DVec3::new(0.5, 64.0, 0.5), DVec3::new(1000.5, 64.0, 0.5)];
        let mut mobs: Vec<Mob> =
            (0..70).map(|i| Mob::new(MobKind::Zombie, DVec3::new(i as f64 * 0.1, 64.0, 0.5), 0.0)).collect();
        let mut named = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        named.persistent = true;
        mobs.push(named);
        let state = SpawnState::new(&players, &mobs, |_| true);
        assert_eq!(state.chunk_count, 578);
        assert_eq!(state.count(MobCategory::Monster), 70);
        // Two players double the global cap; the first player's own cap is full.
        assert!(state.global_room(MobCategory::Monster));
        assert!(!state.local_room(MobCategory::Monster, IVec2::new(0, 0)));
        assert!(state.local_room(MobCategory::Monster, IVec2::new(62, 0)));
        assert!(state.local_room(MobCategory::Creature, IVec2::new(0, 0)));
    }

    #[test]
    fn categories_match_the_reference_registrations() {
        assert_eq!(MobKind::Villager.category(), MobCategory::Misc);
        assert_eq!(MobKind::WanderingTrader.category(), MobCategory::Creature);
        assert_eq!(MobKind::Cod.category(), MobCategory::WaterAmbient);
        assert_eq!(MobKind::GlowSquid.category(), MobCategory::UndergroundWaterCreature);
        assert_eq!(MobKind::Guardian.category(), MobCategory::Monster);
        assert_eq!(MobKind::Strider.category(), MobCategory::Creature);
        assert_eq!(MobCategory::WaterAmbient.despawn_distance(), 64.0);
    }
}
