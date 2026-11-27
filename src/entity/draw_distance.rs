//! How far entities, beacon beams and conduits are drawn (v1.0 Track 2),
//! following Java 26.3 instead of building everything out to the fog's end.
//!
//! Java draws an entity only when two tests pass:
//!
//! - **The client has it.** The integrated server sends an entity to a player
//!   while their horizontal distance is at most
//!   `min(clientTrackingRange * 16, viewDistance * 16)` blocks
//!   (`ChunkMap.TrackedEntity.updatePlayer`), with each type's
//!   `clientTrackingRange` from `EntityTypes` ([`java_tracking_chunks`]).
//! - **The renderer wants it.** `Entity.shouldRenderAtSqrDistance`: the
//!   squared camera distance is below `(size * 64 * viewScale)^2`, where
//!   `size` is the mean side of the bounding box (`AABB.getSize`) and
//!   `viewScale` is `clamp(renderDistance / 8, 1, 2.5) * entityDistanceScaling`
//!   (`LevelExtractor.extractVisibleEntities`). Item frames use 16 for the
//!   size, armor stands `4 * size` without `viewScale`, and phantoms always
//!   pass.
//!
//! Beacon beams draw within `renderDistance * 16` blocks horizontally
//! (`BeaconRenderer.getViewDistance`); conduits keep
//! `BlockEntityRenderer.getViewDistance`'s 64 blocks.
//!
//! On top of Java's tests, nothing the fog hides completely is built: Java
//! would draw it, but in the fog's colour, so dropping it changes no pixel.

use glam::{DVec3, Vec3};

use super::MobKind;

/// `Options.entityDistanceScaling` as Java's default Fancy preset sets it
/// (`GraphicsPreset.FANCY`). VoxelCraft has no Entity Distance option.
pub const ENTITY_DISTANCE_SCALING: f64 = 1.0;
/// `BlockEntityRenderer.getViewDistance`, which `ConduitRenderer` keeps.
pub const CONDUIT_VIEW_DISTANCE: f64 = 64.0;
/// `Entity.shouldRenderAtSqrDistance`'s blocks per unit of size.
const BLOCKS_PER_SIZE: f64 = 64.0;

/// One view's draw distances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawDistance {
    /// The player the server tracks entities for (the view's own player).
    player: DVec3,
    /// Render distance in Java's 16-block chunks; `None` turns Java's
    /// tests off and leaves only the fog (see [`DrawDistance::fog_only`]).
    chunks: Option<i32>,
    view_scale: f64,
    /// `FogData`'s environmental end (spherical distance) and
    /// render-distance end (cylindrical distance).
    environmental_end: f32,
    render_distance_end: f32,
}

impl DrawDistance {
    /// Java's distances for a world drawn `render_distance` storage chunks
    /// (32 blocks each) around `player`, under a fog that ends at
    /// `environmental_end` (spherical) and `render_distance_end`
    /// (cylindrical).
    pub fn new(player: DVec3, render_distance: i32, environmental_end: f32, render_distance_end: f32) -> Self {
        let chunks = render_distance * 2;
        Self { player, chunks: Some(chunks), view_scale: view_scale(chunks), environmental_end, render_distance_end }
    }

    /// Everything within `max_dist` of the camera: the test before v1.0,
    /// kept for fixtures and tools that draw without a world view.
    pub fn fog_only(max_dist: f32) -> Self {
        Self {
            player: DVec3::ZERO,
            chunks: None,
            view_scale: f64::INFINITY,
            environmental_end: max_dist,
            render_distance_end: f32::INFINITY,
        }
    }

    /// `Entity.viewScale` for this view.
    pub fn view_scale(&self) -> f64 {
        self.view_scale
    }

    /// Whether the fog hides everything within `radius` of the
    /// camera-relative point `rel` (`fog.glsl`'s distances).
    pub fn fogged(&self, rel: Vec3, radius: f32) -> bool {
        let spherical = rel.length() - radius;
        let cylindrical = Vec3::new(rel.x, 0.0, rel.z).length().max(rel.y.abs()) - radius;
        spherical >= self.environmental_end || cylindrical >= self.render_distance_end
    }

    /// `ChunkMap.TrackedEntity.updatePlayer`: whether the server sends an
    /// entity at `pos` with this tracking range (Java chunks).
    pub fn tracked(&self, pos: DVec3, tracking_chunks: u8) -> bool {
        let Some(chunks) = self.chunks else { return true };
        let range = f64::from(i32::from(tracking_chunks).min(chunks) * 16);
        let d = (self.player - pos).with_y(0.0);
        d.length_squared() <= range * range
    }

    /// `Entity.shouldRenderAtSqrDistance` for a bounding box whose mean side
    /// is `size`: within `size * 64 * viewScale` of the camera.
    pub fn renders(&self, rel: DVec3, size: f64) -> bool {
        self.within(rel, size * BLOCKS_PER_SIZE * self.view_scale)
    }

    /// Within `distance` of the camera (`distance < limit`, as Java compares).
    pub fn within(&self, rel: DVec3, distance: f64) -> bool {
        self.chunks.is_none() || rel.length_squared() < distance * distance
    }

    /// Both of Java's tests and the fog's for an ordinary entity.
    pub fn entity(&self, pos: DVec3, rel: DVec3, size: f64, tracking_chunks: u8, radius: f32) -> bool {
        self.tracked(pos, tracking_chunks) && self.renders(rel, size) && !self.fogged(rel.as_vec3(), radius)
    }

    /// A mob of `kind` whose current box has half width `half_width` and
    /// height `height`, at `pos` (`rel` from the camera).
    pub fn mob(&self, kind: MobKind, pos: DVec3, rel: DVec3, half_width: f64, height: f64, radius: f32) -> bool {
        if !self.tracked(pos, mob_tracking_chunks(kind)) || self.fogged(rel.as_vec3(), radius) {
            return false;
        }
        // `Phantom.shouldRenderAtSqrDistance` always returns true.
        kind == MobKind::Phantom || self.renders(rel, box_size(half_width * 2.0, height, half_width * 2.0))
    }

    /// `BeaconRenderer.shouldRender`: the base cell's centre within
    /// `renderDistance * 16` blocks of the camera, horizontally.
    pub fn beacon(&self, base_centre: DVec3, camera: DVec3) -> bool {
        let rel = base_centre - camera;
        let horizontal = rel.with_y(0.0);
        let near = self.chunks.is_none_or(|c| horizontal.length_squared() < f64::from(c * 16).powi(2));
        // The beam is a column: only its horizontal distance can take it
        // past the fog.
        near && !self.fogged(horizontal.as_vec3(), 1.0)
    }

    /// `BlockEntityRenderer.shouldRender` for a conduit: its cell's centre
    /// within 64 blocks.
    pub fn conduit(&self, centre: DVec3, camera: DVec3) -> bool {
        let rel = centre - camera;
        self.within(rel, CONDUIT_VIEW_DISTANCE) && !self.fogged(rel.as_vec3(), 1.0)
    }
}

impl From<f32> for DrawDistance {
    fn from(max_dist: f32) -> Self {
        Self::fog_only(max_dist)
    }
}

/// `LevelExtractor`'s `Entity.setViewScale` for a render distance in Java
/// chunks, with Java's default Entity Distance.
pub fn view_scale(java_chunks: i32) -> f64 {
    (f64::from(java_chunks) / 8.0).clamp(1.0, 2.5) * ENTITY_DISTANCE_SCALING
}

/// `AABB.getSize`: the mean of the box's three sides.
pub fn box_size(x: f64, y: f64, z: f64) -> f64 {
    (x + y + z) / 3.0
}

/// A mob kind's `clientTrackingRange`, cached per kind.
pub fn mob_tracking_chunks(kind: MobKind) -> u8 {
    static TABLE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = vec![DEFAULT_TRACKING; MobKind::ALL.len()];
        for k in MobKind::ALL {
            if let Some(slot) = table.get_mut(k as usize) {
                *slot = java_tracking_chunks(&java_name(k)).unwrap_or(DEFAULT_TRACKING);
            }
        }
        table
    });
    table.get(kind as usize).copied().unwrap_or(DEFAULT_TRACKING)
}

/// A mob kind's Java entity id (`zombified piglin` → `zombified_piglin`).
pub fn java_name(kind: MobKind) -> String {
    kind.name().replace(' ', "_")
}

/// `EntityType.Builder`'s default `clientTrackingRange`.
pub const DEFAULT_TRACKING: u8 = 5;

/// `clientTrackingRange` of a Java 26.3 entity type, from `EntityTypes`
/// (types that keep the builder's default, such as the dolphin, are listed
/// with 5).
pub fn java_tracking_chunks(id: &str) -> Option<u8> {
    TRACKING.binary_search_by(|(name, _)| name.cmp(&id)).ok().map(|i| TRACKING[i].1)
}

/// Java 26.3's `clientTrackingRange` by entity id, sorted by id. Read from
/// `EntityTypes` in the pinned client jar (`CLIENT_SHA1` e877b6a0…).
pub const TRACKING: [(&str, u8); 161] = [
    ("acacia_boat", 10),
    ("acacia_chest_boat", 10),
    ("allay", 8),
    ("area_effect_cloud", 10),
    ("armadillo", 10),
    ("armor_stand", 10),
    ("arrow", 4),
    ("axolotl", 10),
    ("bamboo_chest_raft", 10),
    ("bamboo_raft", 10),
    ("bat", 5),
    ("bee", 8),
    ("birch_boat", 10),
    ("birch_chest_boat", 10),
    ("blaze", 8),
    ("block_display", 10),
    ("bogged", 8),
    ("breeze", 10),
    ("breeze_wind_charge", 4),
    ("camel", 10),
    ("camel_husk", 10),
    ("cat", 8),
    ("cave_spider", 8),
    ("cherry_boat", 10),
    ("cherry_chest_boat", 10),
    ("chest_minecart", 8),
    ("chicken", 10),
    ("cod", 4),
    ("command_block_minecart", 8),
    ("copper_golem", 10),
    ("cow", 10),
    ("creaking", 8),
    ("creeper", 8),
    ("cushion", 10),
    ("dark_oak_boat", 10),
    ("dark_oak_chest_boat", 10),
    ("dolphin", 5),
    ("donkey", 10),
    ("dragon_fireball", 4),
    ("drowned", 8),
    ("egg", 4),
    ("elder_guardian", 10),
    ("end_crystal", 16),
    ("ender_dragon", 10),
    ("ender_pearl", 4),
    ("enderman", 8),
    ("endermite", 8),
    ("evoker", 8),
    ("evoker_fangs", 6),
    ("experience_bottle", 4),
    ("experience_orb", 6),
    ("eye_of_ender", 4),
    ("falling_block", 10),
    ("fireball", 4),
    ("firework_rocket", 4),
    ("fishing_bobber", 4),
    ("fox", 8),
    ("frog", 10),
    ("furnace_minecart", 8),
    ("ghast", 10),
    ("giant", 10),
    ("glow_item_frame", 10),
    ("glow_squid", 10),
    ("goat", 10),
    ("guardian", 8),
    ("happy_ghast", 10),
    ("hoglin", 8),
    ("hopper_minecart", 8),
    ("horse", 10),
    ("husk", 8),
    ("illusioner", 8),
    ("interaction", 10),
    ("iron_golem", 10),
    ("item", 6),
    ("item_display", 10),
    ("item_frame", 10),
    ("jungle_boat", 10),
    ("jungle_chest_boat", 10),
    ("leash_knot", 10),
    ("lightning_bolt", 16),
    ("lingering_potion", 4),
    ("llama", 10),
    ("llama_spit", 4),
    ("magma_cube", 8),
    ("mangrove_boat", 10),
    ("mangrove_chest_boat", 10),
    ("mannequin", 32),
    ("marker", 0),
    ("minecart", 8),
    ("mooshroom", 10),
    ("mule", 8),
    ("nautilus", 10),
    ("oak_boat", 10),
    ("oak_chest_boat", 10),
    ("ocelot", 10),
    ("ominous_item_spawner", 8),
    ("painting", 10),
    ("pale_oak_boat", 10),
    ("pale_oak_chest_boat", 10),
    ("panda", 10),
    ("parched", 8),
    ("parrot", 8),
    ("phantom", 8),
    ("pig", 10),
    ("piglin", 8),
    ("piglin_brute", 8),
    ("pillager", 8),
    ("player", 32),
    ("polar_bear", 10),
    ("poplar_boat", 10),
    ("poplar_chest_boat", 10),
    ("pufferfish", 4),
    ("rabbit", 8),
    ("ravager", 10),
    ("salmon", 4),
    ("sheep", 10),
    ("shulker", 10),
    ("shulker_bullet", 8),
    ("silverfish", 8),
    ("skeleton", 8),
    ("skeleton_horse", 10),
    ("slime", 10),
    ("small_fireball", 4),
    ("sniffer", 10),
    ("snow_golem", 8),
    ("snowball", 4),
    ("spawner_minecart", 8),
    ("spectral_arrow", 4),
    ("spider", 8),
    ("splash_potion", 4),
    ("spruce_boat", 10),
    ("spruce_chest_boat", 10),
    ("squid", 8),
    ("stray", 8),
    ("strider", 10),
    ("sulfur_cube", 10),
    ("tadpole", 10),
    ("text_display", 10),
    ("tnt", 10),
    ("tnt_minecart", 8),
    ("trader_llama", 10),
    ("trident", 4),
    ("tropical_fish", 4),
    ("turtle", 10),
    ("vex", 8),
    ("villager", 10),
    ("vindicator", 8),
    ("wandering_trader", 10),
    ("warden", 16),
    ("wind_charge", 4),
    ("witch", 8),
    ("wither", 10),
    ("wither_skeleton", 8),
    ("wither_skull", 4),
    ("wolf", 10),
    ("zoglin", 8),
    ("zombie", 8),
    ("zombie_horse", 10),
    ("zombie_nautilus", 10),
    ("zombie_villager", 8),
    ("zombified_piglin", 8),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tracking_table_is_sorted_and_every_mob_has_its_java_range() {
        assert!(TRACKING.windows(2).all(|w| w[0].0 < w[1].0), "sorted by id for the binary search");
        for kind in MobKind::ALL {
            let id = java_name(kind);
            let range = java_tracking_chunks(&id).unwrap_or_else(|| panic!("{id} is not a Java 26.3 entity type"));
            assert_eq!(mob_tracking_chunks(kind), range, "{id}");
        }
        // Spot checks against `EntityTypes`.
        assert_eq!(java_tracking_chunks("zombie"), Some(8));
        assert_eq!(java_tracking_chunks("cow"), Some(10));
        assert_eq!(java_tracking_chunks("warden"), Some(16));
        assert_eq!(java_tracking_chunks("dolphin"), Some(DEFAULT_TRACKING));
        assert_eq!(java_tracking_chunks("experience_orb"), Some(6));
    }

    #[test]
    fn view_scale_follows_the_render_distance_like_java() {
        // `clamp(renderDistance / 8, 1, 2.5)`, in Java chunks.
        assert_eq!(view_scale(4), 1.0);
        assert_eq!(view_scale(8), 1.0);
        assert_eq!(view_scale(12), 1.5);
        assert_eq!(view_scale(16), 2.0);
        assert_eq!(view_scale(20), 2.5);
        assert_eq!(view_scale(48), 2.5);
    }

    #[test]
    fn mobs_draw_within_javas_tracking_and_render_distances() {
        // Render distance 8 (16 Java chunks): viewScale 2. A zombie
        // (0.6 x 1.95 x 0.6, mean side 1.05) renders within 134.4 blocks,
        // but the server tracks it only within 8 chunks (128 blocks).
        let fog = 10_000.0;
        let d = DrawDistance::new(DVec3::ZERO, 8, fog, fog);
        let zombie = |x: f64| d.mob(MobKind::Zombie, DVec3::new(x, 0.0, 0.0), DVec3::new(x, 0.0, 0.0), 0.3, 1.95, 1.5);
        assert!(zombie(127.9));
        assert!(!zombie(128.1));
        // Straight up, tracking is horizontal: only the render test limits it.
        let up = |y: f64| d.mob(MobKind::Zombie, DVec3::new(0.0, y, 0.0), DVec3::new(0.0, y, 0.0), 0.3, 1.95, 1.5);
        assert!(up(134.3));
        assert!(!up(134.5));
        // A cow (10 chunks, mean side 1.0667) renders within 136.5 blocks,
        // short of its 160-block tracking.
        let cow = |x: f64| d.mob(MobKind::Cow, DVec3::new(x, 0.0, 0.0), DVec3::new(x, 0.0, 0.0), 0.45, 1.4, 1.5);
        assert!(cow(136.4));
        assert!(!cow(136.6));
        // A phantom only needs tracking.
        let phantom =
            |x: f64| d.mob(MobKind::Phantom, DVec3::new(x, 60.0, 0.0), DVec3::new(x, 60.0, 0.0), 0.45, 0.5, 1.5);
        assert!(phantom(127.0));
        assert!(!phantom(129.0));
        // Render distance 2 (4 Java chunks): tracking stops at 64 blocks.
        let near = DrawDistance::new(DVec3::ZERO, 2, fog, fog);
        assert!(!near.mob(MobKind::Cow, DVec3::new(65.0, 0.0, 0.0), DVec3::new(65.0, 0.0, 0.0), 0.45, 1.4, 1.5));
    }

    #[test]
    fn tracking_follows_the_player_and_rendering_the_camera() {
        let fog = 10_000.0;
        // A third-person camera 4 blocks behind the player.
        let d = DrawDistance::new(DVec3::ZERO, 8, fog, fog);
        let pos = DVec3::new(127.0, 0.0, 0.0);
        let camera = DVec3::new(-4.0, 0.0, 0.0);
        assert!(d.entity(pos, pos - camera, 1.05, 8, 1.0));
        let far = DVec3::new(129.0, 0.0, 0.0);
        assert!(!d.entity(far, far - DVec3::new(4.0, 0.0, 0.0), 1.05, 8, 1.0));
    }

    #[test]
    fn nothing_the_fog_hides_is_built() {
        // Render-distance fog ends at 256 blocks of cylindrical distance.
        let d = DrawDistance::new(DVec3::ZERO, 8, 1024.0, 256.0);
        assert!(!d.fogged(Vec3::new(200.0, 0.0, 0.0), 1.0));
        assert!(d.fogged(Vec3::new(258.0, 0.0, 0.0), 1.0));
        // Diagonally the spherical distance passes 256 well before the
        // cylindrical one does.
        assert!(!d.fogged(Vec3::new(180.0, 0.0, 180.0), 1.0));
        assert!(d.fogged(Vec3::new(10.0, 300.0, 0.0), 1.0));
        // The environmental fog is spherical.
        let thick = DrawDistance::new(DVec3::ZERO, 8, 96.0, 256.0);
        assert!(thick.fogged(Vec3::new(60.0, 60.0, 60.0), 1.0));
    }

    #[test]
    fn beacon_beams_and_conduits_use_their_renderers_distances() {
        let d = DrawDistance::new(DVec3::ZERO, 8, 1024.0, 256.0);
        // `renderDistance * 16`: 256 blocks at render distance 8, horizontal only.
        assert!(d.beacon(DVec3::new(255.5, -50.0, 0.0), DVec3::ZERO));
        assert!(!d.beacon(DVec3::new(256.5, 0.0, 0.0), DVec3::ZERO));
        let rd4 = DrawDistance::new(DVec3::ZERO, 4, 1024.0, 128.0);
        assert!(!rd4.beacon(DVec3::new(130.0, 0.0, 0.0), DVec3::ZERO));
        // 64 blocks, spherical, for conduits.
        assert!(d.conduit(DVec3::new(40.0, 40.0, 0.0), DVec3::ZERO));
        assert!(!d.conduit(DVec3::new(46.0, 46.0, 0.0), DVec3::ZERO));
    }

    #[test]
    fn fog_only_keeps_the_old_sphere() {
        let d = DrawDistance::from(64.0);
        assert!(d.mob(MobKind::Zombie, DVec3::new(500.0, 0.0, 0.0), DVec3::new(60.0, 0.0, 0.0), 0.3, 1.95, 1.5));
        assert!(!d.mob(MobKind::Zombie, DVec3::ZERO, DVec3::new(70.0, 0.0, 0.0), 0.3, 1.95, 1.5));
    }
}
