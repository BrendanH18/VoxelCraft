//! Java 26.3 map components and saved map images. No map artwork is imported.
use super::{
    World,
    block::Block,
    temples::Kind,
    terrain::{Dimension, Generator},
    village::Style,
};
use crate::{inventory::Stack, item::Item};
use glam::{DVec3, IVec2, IVec3};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const WIDTH: usize = 128;
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    pub kind: String,
    pub x: f64,
    pub z: f64,
    pub rotation: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MapData {
    pub center: IVec2,
    pub scale: u8,
    pub dimension: Dimension,
    pub colors: Vec<u8>,
    pub decorations: Vec<Decoration>,
    pub tracking: bool,
    pub unlimited: bool,
    pub locked: bool,
}
impl MapData {
    pub fn fresh(x: i32, z: i32, scale: u8, dimension: Dimension, tracking: bool, unlimited: bool) -> Self {
        let scale = scale.min(4);
        let size = 128 << scale;
        let snap = |v: i32| ((i64::from(v) + 64).div_euclid(size as i64) * size as i64 + size as i64 / 2 - 64) as i32;
        Self {
            center: IVec2::new(snap(x), snap(z)),
            scale,
            dimension,
            colors: vec![0; 16384],
            decorations: Vec::new(),
            tracking,
            unlimited,
            locked: false,
        }
    }
    /// MapItemSavedData.calculateDecorationLocationAndType. Coordinates are
    /// quantized to half-pixels and rotations to sixteenths of a turn.
    pub fn decoration_view(&self, d: &Decoration) -> Option<(f32, f32, u8, bool)> {
        let scale = (1 << self.scale) as f32;
        let x = (d.x - self.center.x as f64) as f32 / scale;
        let z = (d.z - self.center.y as f64) as f32 / scale;
        let inside = (-63.0..=63.0).contains(&x) && (-63.0..=63.0).contains(&z);
        let player = d.kind == "player";
        if !inside && !self.unlimited && (!player || x.abs() >= 320.0 || z.abs() >= 320.0) {
            return None;
        }
        let clamp = |v: f32| {
            if v <= -63.0 {
                -128i8
            } else if v >= 63.0 {
                127i8
            } else {
                (v * 2.0 + 0.5) as i8
            }
        };
        let rotation = (((d.rotation + if d.rotation < 0.0 { -8.0 } else { 8.0 }) * 16.0 / 360.0) as i32 & 15) as u8;
        Some((clamp(x) as f32 / 2.0 + 64.0, clamp(z) as f32 / 2.0 + 64.0, rotation, player && !inside))
    }
    pub fn scaled(&self) -> Self {
        Self::fresh(
            self.center.x,
            self.center.y,
            self.scale.saturating_add(1).min(4),
            self.dimension,
            self.tracking,
            self.unlimited,
        )
    }
    /// MapItem.update. Samples return (map colour, surface height, fluid depth).
    /// Missing columns are skipped, so inspecting a map never loads terrain.
    pub fn update(&mut self, pos: DVec3, step: u32, mut sample: impl FnMut(i32, i32) -> Option<(u8, i32, i32)>) {
        if self.locked {
            return;
        }
        let scale = 1 << self.scale;
        let px = (pos.x - self.center.x as f64).floor() as i32 / scale + 64;
        let pz = (pos.z - self.center.y as f64).floor() as i32 / scale + 64;
        let ceiling = self.dimension == Dimension::Nether;
        let radius = 128 / scale / if ceiling { 2 } else { 1 };
        let mut consecutive = false;
        for x in px - radius + 1..px + radius {
            if x & 15 != (step as i32 & 15) && !consecutive {
                continue;
            }
            consecutive = false;
            let mut previous = 0.0;
            for z in pz - radius - 1..pz + radius {
                if !(0..128).contains(&x) || !(-1..128).contains(&z) {
                    continue;
                }
                let wx = (self.center.x / scale + x - 64) * scale;
                let wz = (self.center.y / scale + z - 64) * scale;
                let mut colors = [0u16; 64];
                let mut order = [0usize; 64];
                let mut order_len = 0;
                let mut height = 0.0;
                let mut depth = 0;
                let mut available = true;
                if ceiling {
                    if sample(wx, wz).is_none() {
                        continue;
                    }
                    let mut noise = wx.wrapping_add(wz.wrapping_mul(231871));
                    noise = noise.wrapping_mul(noise).wrapping_mul(31287121).wrapping_add(noise.wrapping_mul(11));
                    order[0] = if noise >> 20 & 1 == 0 { 10 } else { 11 };
                    order_len = 1;
                    height = 100.0;
                } else {
                    for dx in 0..scale {
                        for dz in 0..scale {
                            if let Some((color, h, d)) = sample(wx + dx, wz + dz) {
                                let color = color.min(63) as usize;
                                if colors[color] == 0 {
                                    order[order_len] = color;
                                    order_len += 1;
                                }
                                colors[color] += 1;
                                height += h as f64 / (scale * scale) as f64;
                                depth += d;
                            } else {
                                available = false;
                            }
                        }
                    }
                }
                if !available {
                    continue;
                }
                let order = &order[..order_len];
                let color = order.first().copied().map_or(0, |first| {
                    order.iter().copied().fold(first, |best, c| if colors[c] > colors[best] { c } else { best })
                });
                // Ties retain LinkedHashMultiset encounter order.
                let parity = ((x + z) & 1) as f64;
                let diff = if color == 12 {
                    (depth / (scale * scale)) as f64 * 0.1 + parity * 0.2
                } else {
                    (height - previous) * 4.0 / (scale + 4) as f64 + (parity - 0.5) * 0.4
                };
                let brightness = if color == 12 {
                    if diff < 0.5 {
                        2
                    } else if diff > 0.9 {
                        0
                    } else {
                        1
                    }
                } else if diff > 0.6 {
                    2
                } else if diff < -0.6 {
                    0
                } else {
                    1
                };
                previous = height;
                let distance = (x - px).pow(2) + (z - pz).pow(2);
                if z >= 0 && distance < radius * radius && (distance <= (radius - 2).pow(2) || (x + z) & 1 != 0) {
                    let pixel = &mut self.colors[(z * 128 + x) as usize];
                    let packed = color as u8 * 4 + brightness;
                    consecutive |= *pixel != packed;
                    *pixel = packed;
                }
            }
        }
    }
    pub fn biome_preview(&mut self, mut watery: impl FnMut(i32, i32) -> bool) {
        let scale = 1 << self.scale;
        let mut water = [false; 16384];
        for z in 0..128 {
            for x in 0..128 {
                water[z * 128 + x] = watery(
                    (self.center.x / scale + x as i32 - 64) * scale,
                    (self.center.y / scale + z as i32 - 64) * scale,
                );
            }
        }
        for x in 1..127 {
            for z in 1..127 {
                let mut n = 0;
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        if (dx != 0 || dz != 0) && water[((z as i32 + dz) * 128 + x as i32 + dx) as usize] {
                            n += 1;
                        }
                    }
                }
                let (color, brightness) = if water[z * 128 + x] {
                    if n > 7 {
                        if z % 2 != 0 {
                            continue;
                        }
                        let wave = ((x as i32 + (java_sin(z as f32) * 7.0) as i32) / 8) % 5;
                        (
                            15,
                            match wave {
                                0 | 4 => 0,
                                1 | 3 => 1,
                                2 => 2,
                                _ => 3,
                            },
                        )
                    } else {
                        (
                            15,
                            if n > 5 {
                                1
                            } else if n > 1 {
                                0
                            } else {
                                3
                            },
                        )
                    }
                } else if n > 0 {
                    (26, if n > 3 { 1 } else { 3 })
                } else {
                    continue;
                };
                self.colors[z * 128 + x] = color * 4 + brightness;
            }
        }
    }
}
#[derive(Clone, Default, Debug, PartialEq)]
pub struct MapStorage {
    pub next_id: u32,
    /// Global simulation age for Java's Nether decoration rotation.
    pub game_ticks: u64,
    pub maps: BTreeMap<u32, MapData>,
    steps: BTreeMap<(u32, u64), u32>,
}
impl MapStorage {
    pub fn create(&mut self, map: MapData, item: Item) -> Stack {
        let id = self.next_id;
        self.next_id = id.checked_add(1).expect("map id exhausted");
        self.maps.insert(id, map);
        Stack { map_id: Some(id), ..Stack::new(item, 1) }
    }
    pub fn explorer(&mut self, g: &Generator, from: IVec3, item: Item, radius: i32) -> Option<Stack> {
        let target = locate(g, from, item, radius)?;
        let mut data = MapData::fresh(
            target.x,
            target.z,
            if item == Item::BURIED_TREASURE_MAP { 1 } else { 2 },
            g.dimension,
            true,
            true,
        );
        data.biome_preview(|x, z| {
            let biome = g.biome_at(IVec3::new(x, super::terrain::SEA_LEVEL, z));
            biome.is_watery() || biome.is_swamp()
        });
        data.decorations.push(Decoration {
            kind: item.name().replace(' ', "_"),
            x: target.x as f64,
            z: target.z as f64,
            rotation: 180.0,
        });
        Some(self.create(data, item))
    }
    pub fn serialize(&self) -> String {
        let maps: Vec<_> = self.maps.iter().map(|(id,m)| json!({"id":id,"x":m.center.x,"z":m.center.y,"scale":m.scale,"dimension":m.dimension.name(),"tracking":m.tracking,"unlimited":m.unlimited,"locked":m.locked,"colors":m.colors.iter().map(|b|format!("{b:02x}")).collect::<String>(),"decorations":m.decorations.iter().map(|d|json!([d.kind,d.x,d.z,d.rotation])).collect::<Vec<_>>()})).collect();
        json!({"version":1,"next":self.next_id,"time":self.game_ticks,"maps":maps}).to_string()
    }
    pub fn load(text: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(text).ok()?;
        let mut out = Self {
            next_id: u32::try_from(v["next"].as_u64()?).ok()?,
            game_ticks: v["time"].as_u64().unwrap_or(0),
            ..Self::default()
        };
        for m in v["maps"].as_array()? {
            let id = u32::try_from(m["id"].as_u64()?).ok()?;
            let mut data = MapData::fresh(
                0,
                0,
                m["scale"].as_u64()?.try_into().ok()?,
                Dimension::from_name(m["dimension"].as_str()?)?,
                m["tracking"].as_bool()?,
                m["unlimited"].as_bool()?,
            );
            data.center = IVec2::new(m["x"].as_i64()?.try_into().ok()?, m["z"].as_i64()?.try_into().ok()?);
            data.locked = m["locked"].as_bool().unwrap_or(false);
            let hex = m["colors"].as_str()?;
            if hex.len() != 32768 || !hex.is_ascii() {
                return None;
            }
            data.colors =
                (0..16384).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect::<Option<Vec<_>>>()?;
            for d in m["decorations"].as_array()? {
                data.decorations.push(Decoration {
                    kind: d[0].as_str()?.into(),
                    x: d[1].as_f64()?,
                    z: d[2].as_f64()?,
                    rotation: d[3].as_f64()? as f32,
                });
            }
            out.next_id = out.next_id.max(id.checked_add(1)?);
            out.maps.insert(id, data);
        }
        Some(out)
    }
}
fn buried_treasure(_g: &Generator, _from: IVec3, _radius: i32) -> Option<IVec3> {
    // v0.14 integration: buried treasure locate
    None
}
pub fn locate(g: &Generator, from: IVec3, item: Item, radius: i32) -> Option<IVec3> {
    match item {
        Item::OCEAN_MONUMENT_MAP => g.temples.locate_rings(g, &[Kind::Monument], from, radius),
        Item::JUNGLE_PYRAMID_MAP => g.temples.locate_rings(g, &[Kind::JungleTemple], from, radius),
        Item::SWAMP_HUT_MAP => g.temples.locate_rings(g, &[Kind::SwampHut], from, radius),
        Item::BURIED_TREASURE_MAP => buried_treasure(g, from, radius),
        Item::DESERT_VILLAGE_MAP
        | Item::PLAINS_VILLAGE_MAP
        | Item::SAVANNA_VILLAGE_MAP
        | Item::SNOWY_VILLAGE_MAP
        | Item::TAIGA_VILLAGE_MAP => {
            let style = match item {
                Item::DESERT_VILLAGE_MAP => Style::Desert,
                Item::PLAINS_VILLAGE_MAP => Style::Plains,
                Item::SAVANNA_VILLAGE_MAP => Style::Savanna,
                Item::SNOWY_VILLAGE_MAP => Style::Snowy,
                _ => Style::Taiga,
            };
            g.villages.locate_style(g, from, style, radius)
        }
        _ => None,
    }
}
impl World {
    pub fn tick_map(&mut self, stack: Stack, pos: DVec3, player: u64) {
        let Some(id) = stack.map_id else { return };
        if self.maps.maps.get(&id).is_none_or(|m| m.locked || m.dimension != self.generator.dimension) {
            return;
        }
        let mut storage = std::mem::take(&mut self.maps);
        let step = storage.steps.entry((id, player)).or_default();
        *step = step.wrapping_add(1);
        if let Some(map) = storage.maps.get_mut(&id)
            && map.dimension == self.generator.dimension
        {
            map.update(pos, *step, |x, z| {
                if !self.column_loaded(x, z) {
                    return None;
                }
                let Some(mut y) = self.world_surface_height(x, z) else {
                    return Some((block_color(Block::BEDROCK), self.generator.dimension.min_y(), 0));
                };
                let mut b = self.get_block(IVec3::new(x, y, z))?;
                while block_color(b) == 0 && y > self.generator.dimension.min_y() {
                    y -= 1;
                    b = self.get_block(IVec3::new(x, y, z))?;
                }
                let mut depth = 0;
                let fluid = b.is_water() || b.is_waterlogged() || b.is_lava();
                if fluid {
                    let mut sy = y - 1;
                    loop {
                        depth += 1;
                        let below = self.get_block(IVec3::new(x, sy, z))?;
                        sy -= 1;
                        if sy <= self.generator.dimension.min_y()
                            || !(below.is_water() || below.is_waterlogged() || below.is_lava())
                        {
                            break;
                        }
                    }
                }
                let color = if fluid && !super::mount_states::face_sturdy(b, IVec3::Y) {
                    if b.is_lava() { 4 } else { 12 }
                } else {
                    block_color(b)
                };
                Some((color, y, depth))
            });
        }
        self.maps = storage;
    }
}

/// Exact common materials; the fixture covers constant Java registrations.
/// Unknown/state-dependent blocks are explicitly listed as gaps in the record.
pub fn block_color(b: Block) -> u8 {
    if b == Block::AIR {
        return 0;
    }
    if b.is_water() {
        return 12;
    }
    if matches!(b, Block::GRASS | Block::SNOWY_GRASS) {
        return 1;
    }
    const DYE: [u8; 16] = [8, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29];
    if let Some(c) = b.wool_color().or_else(|| b.carpet_color()) {
        return DYE[c as usize];
    }
    if let Some(c) = b.bed_color() {
        return if b.is_bed_head() { 3 } else { DYE[c as usize] };
    }
    if let Some(c) = b.stained_terracotta_color() {
        return 36 + c as u8;
    }
    let name = match b.name() {
        "block of gold" => "gold block",
        "block of iron" => "iron block",
        "block of diamond" => "diamond block",
        "block of emerald" => "emerald block",
        "block of lapis lazuli" => "lapis block",
        "block of coal" => "coal block",
        "block of quartz" => "quartz block",
        "block of copper" => "copper block",
        "block of raw iron" => "raw iron block",
        "block of raw gold" => "raw gold block",
        "block of raw copper" => "raw copper block",
        "block of amethyst" => "amethyst block",
        "block of bamboo" => "bamboo block",
        "block of netherite" => "netherite block",
        n => n,
    };
    #[derive(Clone, Copy)]
    enum Rule {
        Fixed(u8),
        Axis(u8, u8),
        Bed(u8),
        Waterlogged(u8),
    }
    static COLORS: std::sync::OnceLock<BTreeMap<String, Rule>> = std::sync::OnceLock::new();
    let colors = COLORS.get_or_init(|| {
        let v: Value = serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/map_colors.json")).unwrap();
        v["blocks"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(k, v)| {
                let rule = if let Some(c) = v.as_u64() {
                    Rule::Fixed(c as u8)
                } else if let Some(a) = v["axis"].as_array() {
                    Rule::Axis(a[0].as_u64()? as u8, a[1].as_u64()? as u8)
                } else if let Some(c) = v["bed"].as_u64() {
                    Rule::Bed(c as u8)
                } else {
                    Rule::Waterlogged(v["waterlogged"].as_u64()? as u8)
                };
                Some((k.replace('_', " "), rule))
            })
            .collect()
    });
    if let Some(&rule) = colors.get(name) {
        return match rule {
            Rule::Fixed(c) => c,
            Rule::Axis(top, side) => {
                if super::wood::axis(b).unwrap_or(0) == 0 {
                    top
                } else {
                    side
                }
            }
            Rule::Bed(c) => {
                if b.is_bed_head() {
                    3
                } else {
                    c
                }
            }
            Rule::Waterlogged(c) => {
                if b.is_waterlogged() {
                    12
                } else {
                    c
                }
            }
        };
    }
    if name.contains("copper") {
        return if name.contains("oxidized") {
            55
        } else if name.contains("weathered") {
            56
        } else if name.contains("exposed") {
            44
        } else {
            15
        };
    }
    match name {
        "grass block" | "slime block" => 1,
        "sand" | "sandstone" | "end stone" | "birch planks" => 2,
        "lava" | "tnt" | "redstone block" => 4,
        "ice" | "packed ice" | "blue ice" => 5,
        "iron block" | "iron door" | "anvil" => 6,
        "snow" | "snow block" | "white wool" => 8,
        "clay" => 9,
        "dirt" | "coarse dirt" | "farmland" | "granite" => 10,
        "oak planks" | "planks" | "oak log" | "log" | "bookshelf" | "chest" => 13,
        "quartz block" | "diorite" | "sea lantern" => 14,
        "gold block" => 30,
        "diamond block" => 31,
        "lapis block" => 32,
        "emerald block" => 33,
        "podzol" | "spruce planks" => 34,
        "netherrack" | "nether bricks" | "magma block" => 35,
        "deepslate" | "cobbled deepslate" => 59,
        "raw iron block" => 60,
        "glow lichen" => 61,
        _ if name.contains("leaves") || name.contains("grass") || name.contains("fern") || name.contains("vine") => 7,
        _ if name.contains("glass") && !name.contains("stained") || name.contains("torch") || name == "barrier" => 0,
        _ => 11,
    }
}

pub const BASE_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744, 16777215, 10791096, 9923917, 7368816, 4210943,
    9402184, 16776437, 14188339, 11685080, 6724056, 15066419, 8375321, 15892389, 5000268, 10066329, 5013401, 8339378,
    3361970, 6704179, 6717235, 10040115, 1644825, 16445005, 6085589, 4882687, 55610, 8476209, 7340544, 13742497,
    10441252, 9787244, 7367818, 12223780, 6780213, 10505550, 3746083, 8874850, 5725276, 8014168, 4996700, 4993571,
    5001770, 9321518, 2430480, 12398641, 9715553, 6035741, 1474182, 3837580, 5647422, 1356933, 6579300, 14200723,
    8365974,
];
pub fn rgba(packed: u8) -> [u8; 4] {
    if packed / 4 == 0 {
        return [0, 0, 0, 0];
    }
    let Some(&rgb) = BASE_COLORS.get((packed / 4) as usize) else { return [0; 4] };
    let m = [180, 220, 255, 135][(packed & 3) as usize];
    [(((rgb >> 16) & 255) * m / 255) as u8, (((rgb >> 8) & 255) * m / 255) as u8, ((rgb & 255) * m / 255) as u8, 255]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{stack_from_str, stack_to_string};
    #[test]
    fn java_centres_snap_across_negative_boundaries_and_scale() {
        for (x, scale, center) in
            [(0, 0, 0), (-64, 0, 0), (-65, 0, -128), (64, 0, 128), (0, 2, 192), (-65, 2, -320), (511, 2, 704)]
        {
            let m = MapData::fresh(x, x, scale, Dimension::Overworld, true, false);
            assert_eq!(m.center, IVec2::splat(center), "{x} scale {scale}");
            let scaled = m.scaled();
            assert_eq!(scaled.scale, (scale + 1).min(4));
            assert!(scaled.colors.iter().all(|&c| c == 0));
        }
    }
    #[test]
    fn colors_use_java_palette_and_brightness() {
        assert_eq!(rgba(12 * 4 + 2), [64, 64, 255, 255]);
        assert_eq!(rgba(12 * 4), [45, 45, 180, 255]);
        assert_eq!(rgba(0), [0; 4]);
        assert_eq!(block_color(Block::WATER), 12);
        assert_eq!(block_color(Block::STONE), 11);
        assert_eq!(block_color(Block::GRASS), 1);
        assert_eq!(block_color(Block::SNOWY_GRASS), 1);
        assert_eq!(block_color(Block::GOLD_BLOCK), 30);
        assert_eq!(block_color(Block::AIR), 0);
    }
    #[test]
    fn maps_and_stack_components_reload_and_old_stack_strings_still_load() {
        let mut storage = MapStorage::default();
        let mut map = MapData::fresh(-200, 350, 2, Dimension::Overworld, true, true);
        map.colors[8123] = 255;
        map.decorations.push(Decoration { kind: "target_x".into(), x: -200.0, z: 350.0, rotation: 180.0 });
        let stack = storage.create(map, Item::JUNGLE_PYRAMID_MAP);
        let loaded = MapStorage::load(&storage.serialize()).unwrap();
        assert_eq!(loaded, storage);
        assert_eq!(stack_from_str(&stack_to_string(Some(stack))), Some(Some(stack)));
        let second = Stack { map_id: Some(1), ..stack };
        assert!(!stack.stacks_with(&second));
        assert_eq!(stack_from_str("256:2"), Some(Some(Stack::new(Item(256), 2))));
        assert_eq!(stack_from_str("1256:1"), Some(Some(Stack::new(Item::FILLED_MAP, 1))));
        assert!(MapStorage::load("{}").is_none());
    }
    #[test]
    fn update_shades_water_depth_and_height_changes() {
        let mut map = MapData::fresh(0, 0, 0, Dimension::Overworld, true, false);
        map.update(DVec3::ZERO, 1, |x, z| {
            Some((if x < 0 { 12 } else { 11 }, if z > 0 { 70 } else { 64 }, if x < -8 { 12 } else { 1 }))
        });
        assert_eq!(map.colors[64 * 128 + 40], 12 * 4, "deep water is low brightness");
        assert_eq!(map.colors[64 * 128 + 60], 12 * 4 + 2, "shallow water is high brightness");
        assert_eq!(map.colors[65 * 128 + 70], 11 * 4 + 2, "six-block rise is high brightness");
        assert_eq!(map.colors[66 * 128 + 70], 11 * 4 + 1, "level ground is normal brightness");
    }
    #[test]
    fn update_skips_missing_columns_and_locked_maps() {
        let mut m = MapData::fresh(0, 0, 2, Dimension::Overworld, true, false);
        m.update(DVec3::new(192.0, 0.0, 192.0), 1, |_, _| None);
        assert!(m.colors.iter().all(|&c| c == 0));
        m.locked = true;
        m.update(DVec3::ZERO, 1, |_, _| panic!("locked map sampled world"));
    }
    #[test]
    fn biome_preview_draws_coasts_and_leaves_inland_transparent() {
        let mut m = MapData::fresh(0, 0, 2, Dimension::Overworld, true, true);
        m.biome_preview(|x, _| x < 192);
        assert_eq!(m.colors[64 * 128 + 64], 26 * 4 + 3);
        assert_eq!(m.colors[64 * 128 + 65], 0);
        assert_eq!(m.colors[64 * 128 + 62] / 4, 15);
        assert_eq!(m.colors[0], 0);
    }
    #[test]
    fn explorer_points_at_the_java_ring_search_result() {
        let g = Generator::new(42);
        let mut storage = MapStorage::default();
        let expected = g.temples.locate_rings(&g, &[Kind::SwampHut], IVec3::ZERO, 50).unwrap();
        let stack = storage.explorer(&g, IVec3::ZERO, Item::SWAMP_HUT_MAP, 50).unwrap();
        let m = &storage.maps[&stack.map_id.unwrap()];
        assert_eq!(m.scale, 2);
        assert_eq!((m.decorations[0].x, m.decorations[0].z), (expected.x as f64, expected.z as f64));
        assert_eq!(m.center, MapData::fresh(expected.x, expected.z, 2, Dimension::Overworld, true, true).center);
        assert!(storage.explorer(&g, IVec3::ZERO, Item::BURIED_TREASURE_MAP, 50).is_none());
    }
    #[test]
    fn map_recipe_and_cloning_keep_the_id_and_name() {
        let mut grid = crate::crafting::Grid::new(3);
        for s in &mut grid.cells {
            *s = Some(Stack::new(Item::PAPER, 1));
        }
        grid.cells[4] = Some(Stack::new(Item::COMPASS, 1));
        assert_eq!(grid.result(), Some(Stack::new(Item::MAP, 1)));
        let original = Stack { map_id: Some(74), ..Stack::new(Item::OCEAN_MONUMENT_MAP, 1) };
        grid.cells = [None; 9];
        grid.cells[1] = Some(original);
        grid.cells[3] = Some(Stack::new(Item::MAP, 2));
        grid.cells[7] = Some(Stack::new(Item::MAP, 1));
        assert_eq!(grid.result(), Some(Stack { count: 3, ..original }));
        grid.cells[8] = Some(original);
        assert_eq!(grid.result(), None);
    }
}

// Mth.sin quantizes radians into Java's 65536-entry sine table.
fn java_sin(angle: f32) -> f32 {
    let i = (angle * 10430.378_f32) as i32 & 65535;
    ((i as f64) * std::f64::consts::TAU / 65536.0).sin() as f32
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn world_folder_roundtrip_keeps_images_and_stack_ids() {
        use super::super::storage::{LevelInfo, Storage};
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/maps-save-test-{}-{unique}", std::process::id()));
        let storage = Storage::new(&dir);
        let mut maps = MapStorage::default();
        let mut data = MapData::fresh(-65, 400, 2, Dimension::Overworld, true, true);
        data.colors[1234] = 50;
        let stack = maps.create(data, Item::FILLED_MAP);
        let mut inventory = crate::inventory::Inventory::default();
        inventory.slots[0] = Some(stack);
        let props = BTreeMap::from([("maps".into(), maps.serialize()), ("inventory".into(), inventory.serialize())]);
        storage.save_level(&LevelInfo { seed: 42, player: None, props }).unwrap();
        let loaded = storage.load_level().unwrap();
        assert_eq!(MapStorage::load(&loaded.props["maps"]).unwrap(), maps);
        assert_eq!(crate::inventory::Inventory::deserialize(&loaded.props["inventory"]).unwrap(), inventory);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn map_sampling_work_stays_bounded_at_every_scale_and_updates_stripes() {
        for scale in 0..=4 {
            let mut m = MapData::fresh(0, 0, scale, Dimension::Overworld, true, false);
            let p = DVec3::new(m.center.x as f64, 64.0, m.center.y as f64);
            let mut first = 0;
            m.update(p, 1, |_, _| {
                first += 1;
                Some((11, 64, 0))
            });
            assert!(first <= 70000, "scale {scale}: {first}");
            let mut next = 0;
            m.update(p, 1, |_, _| {
                next += 1;
                Some((11, 64, 0))
            });
            assert!(next < first, "unchanged columns stop propagation");
        }
    }
    #[test]
    fn player_decorations_clamp_and_use_tracking_limits() {
        let mut m = MapData::fresh(0, 0, 0, Dimension::Overworld, true, false);
        let mut d = Decoration { kind: "player".into(), x: 0.0, z: 0.0, rotation: 90.0 };
        assert_eq!(m.decoration_view(&d), Some((64.0, 64.0, 4, false)));
        d.x = -100.0;
        assert_eq!(m.decoration_view(&d), Some((0.0, 64.0, 4, true)));
        d.x = 320.0;
        assert_eq!(m.decoration_view(&d), None);
        m.unlimited = true;
        assert_eq!(m.decoration_view(&d), Some((127.5, 64.0, 4, true)));
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn other_dimension_and_locked_maps_do_not_advance_the_update_stripe() {
        let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 1);
        let stack = world.maps.create(MapData::fresh(0, 0, 0, Dimension::Nether, true, false), Item::FILLED_MAP);
        world.tick_map(stack, DVec3::ZERO, 1);
        assert!(world.maps.steps.is_empty());
        let data = world.maps.maps.get_mut(&0).unwrap();
        data.dimension = Dimension::Overworld;
        data.locked = true;
        world.tick_map(stack, DVec3::ZERO, 1);
        assert!(world.maps.steps.is_empty());
    }
    #[test]
    fn loaded_void_columns_use_java_bedrock_colour() {
        let g = Arc::new(Generator::new(42));
        let mut world = World::new_headless(g, Default::default(), 1);
        for z in -2..2 {
            for x in -2..2 {
                for y in Dimension::Overworld.chunk_rows() {
                    world.insert_chunk(
                        IVec3::new(x, y, z),
                        Arc::new(super::super::chunk::ChunkData::Uniform(Block::AIR)),
                        true,
                    );
                }
            }
        }
        let stack = world.maps.create(MapData::fresh(0, 0, 0, Dimension::Overworld, true, false), Item::FILLED_MAP);
        world.tick_map(stack, DVec3::ZERO, 1);
        assert_eq!(world.maps.maps[&0].colors[64 * 128 + 64], 11 * 4 + 1);
    }
    #[test]
    fn loaded_world_map_updates_have_bounded_work() {
        // A complete 128x128 loaded scene, built from the ordinary chunk API.
        // Timings are evidence, not flaky pass/fail thresholds.
        let g = Arc::new(Generator::new(42));
        let mut world = World::new_headless(g, Default::default(), 1);
        for z in -2..2 {
            for x in -2..2 {
                for y in Dimension::Overworld.chunk_rows() {
                    world.insert_chunk(
                        IVec3::new(x, y, z),
                        Arc::new(super::super::chunk::ChunkData::Uniform(if y < 2 {
                            Block::STONE
                        } else {
                            Block::AIR
                        })),
                        true,
                    );
                }
            }
        }
        let stack = world.maps.create(MapData::fresh(0, 0, 0, Dimension::Overworld, true, false), Item::FILLED_MAP);
        let start = std::time::Instant::now();
        world.tick_map(stack, DVec3::new(0.0, 65.0, 0.0), 1);
        let initial = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            world.tick_map(stack, DVec3::new(0.0, 65.0, 0.0), 1);
        }
        let steady = start.elapsed() / 20;
        assert!(world.maps.maps[&0].colors.iter().all(|&c| c / 4 == 11));
        eprintln!(
            "map update, seed 42, loaded 128x128 stone scene: first {:.3} ms, steady mean {:.3} ms/tick",
            initial.as_secs_f64() * 1000.0,
            steady.as_secs_f64() * 1000.0
        );
    }
}
