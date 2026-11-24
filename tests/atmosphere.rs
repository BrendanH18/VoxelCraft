//! v0.18 Track 8: Java 26.3's fog, sky and cloud rules against
//! `fixtures/java/26.3/atmosphere.json` (tools/extract_java_atmosphere_fixtures.py).
//! Each fog environment's distances and colour are worked out here from the
//! fixture's numbers and compared with `voxelcraft::atmosphere::fog`.
use serde_json::Value;
use voxelcraft::{
    atmosphere::{
        self, Clouds, Eye, Local, Medium, RainFog, Weather,
        clouds::{self, Pattern, Relative, View},
    },
    simulation::lighting::SpawnLight,
    world::{biome::Biome, nether_biome::NetherBiome, terrain::Dimension},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/atmosphere.json")).unwrap()
}

fn hex(v: &Value) -> u32 {
    u32::from_str_radix(v.as_str().unwrap().trim_start_matches('#'), 16).unwrap()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * b.abs().max(1.0)
}

fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| close(a[i], b[i]))
}

fn plains_noon() -> Eye {
    Eye {
        dimension: Dimension::Overworld,
        local: Local::resolve(atmosphere::dimension(Dimension::Overworld), atmosphere::overworld_biome(Biome::Plains)),
        precipitation: true,
        day_ticks: 6000.0,
        weather: Weather::default(),
        sky_flash: false,
        render_distance: 256.0,
        cloud_range: 128,
        sky_light: 15,
        camera_y: 100.0,
        forward_x: 0.0,
        medium: Medium::Air,
        spectator: false,
        fire_resistance: false,
        blindness: None,
        darkness: None,
        night_vision: 0.0,
        water_vision: 0.0,
    }
}

#[test]
fn biome_and_dimension_tables_match_the_client() {
    let fx = fixture();
    let biomes = fx["biomes"].as_object().unwrap();
    assert_eq!(biomes.len(), atmosphere::BIOMES.len());
    for (name, b) in biomes {
        let v = atmosphere::biome_visuals(name).unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(v.sky_color, b.get("sky_color").map(hex), "{name} sky");
        assert_eq!(v.fog_color, b.get("fog_color").map(hex), "{name} fog");
        assert_eq!(v.water_fog_color, b.get("water_fog_color").map(hex), "{name} water fog");
        assert_eq!(v.water_fog_end_multiplier, b.get("water_fog_end_distance_multiplier").map_or(1.0, f), "{name}");
        assert_eq!(v.precipitation, b["has_precipitation"].as_bool().unwrap(), "{name} precipitation");
    }
    // Every biome VoxelCraft generates has Java's attributes.
    for biome in Biome::ALL {
        assert!(atmosphere::biome_visuals(biome.name()).is_some(), "{}", biome.name());
    }
    for biome in NetherBiome::ALL {
        let name = format!("{biome:?}");
        let id: String = name
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                let lower = c.to_ascii_lowercase();
                if c.is_ascii_uppercase() && i > 0 { vec!['_', lower] } else { vec![lower] }
            })
            .collect();
        assert_eq!(Some(biome.fog_rgb()), atmosphere::biome_visuals(&id).unwrap().fog_color, "{id}");
    }
    let d = &fx["dimensions"];
    let ow = atmosphere::dimension(Dimension::Overworld);
    assert_eq!(ow.sky_color, hex(&d["overworld"]["sky_color"]));
    assert_eq!(ow.fog_color, hex(&d["overworld"]["fog_color"]));
    assert_eq!(ow.cloud_color, hex(&d["overworld"]["cloud_color"]));
    assert_eq!(ow.cloud_height, f(&d["overworld"]["cloud_height"]));
    assert_eq!(ow.min_y, d["overworld"]["min_y"].as_f64().unwrap() as f32);
    let nether = atmosphere::dimension(Dimension::Nether);
    assert_eq!(
        (nether.fog_start, nether.fog_end),
        (f(&d["the_nether"]["fog_start_distance"]), f(&d["the_nether"]["fog_end_distance"]))
    );
    assert!(d["the_nether"].get("cloud_color").is_none() && nether.cloud_color == 0);
    let end = atmosphere::dimension(Dimension::End);
    assert_eq!(end.fog_color, hex(&d["the_end"]["fog_color"]));
    assert_eq!(end.sky_color, hex(&d["the_end"]["sky_color"]));
    let defaults = &fx["attribute_defaults"];
    assert_eq!(end.fog_end, f(&defaults["fog_end_distance"]));
    assert_eq!(atmosphere::SKY_FOG_END, f(&defaults["sky_fog_end_distance"]));
    assert_eq!(atmosphere::CLOUD_FOG_END, f(&defaults["cloud_fog_end_distance"]));
    assert_eq!(atmosphere::WATER_FOG_START, f(&defaults["water_fog_start_distance"]));
    assert_eq!(atmosphere::WATER_FOG_END, f(&defaults["water_fog_end_distance"]));
    assert_eq!(atmosphere::WATER_FOG_COLOR, hex(&defaults["water_fog_color"]));
}

#[test]
fn day_timeline_tracks_match_the_client() {
    let fx = fixture();
    let tracks = &fx["day_timeline"]["tracks"];
    assert_eq!(fx["day_timeline"]["period_ticks"].as_f64().unwrap(), atmosphere::DAY_TICKS);
    let keys = |name: &str| -> Vec<(u32, u32)> {
        let t = &tracks[name];
        assert_eq!(t["ease"], "linear", "{name}");
        t["keyframes"].as_array().unwrap().iter().map(|k| (k[0].as_u64().unwrap() as u32, hex(&k[1]))).collect()
    };
    assert_eq!(keys("fog_color"), atmosphere::DAY_FOG);
    assert_eq!(keys("sky_color"), atmosphere::DAY_SKY);
    assert_eq!(keys("cloud_color"), atmosphere::DAY_CLOUD);
    assert_eq!(keys("sunrise_sunset_color"), atmosphere::DAY_SUNRISE);
    assert_eq!(keys("sky_light_color"), atmosphere::DAY_SKY_LIGHT_COLOR);
    let factor: Vec<(u32, f32)> = tracks["sky_light_factor"]["keyframes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| (k[0].as_u64().unwrap() as u32, f(&k[1])))
        .collect();
    assert_eq!(factor, atmosphere::DAY_SKY_LIGHT_FACTOR);
    for name in ["fog_color", "sky_color", "cloud_color", "sky_light_color", "sky_light_factor"] {
        assert_eq!(tracks[name]["modifier"], "multiply", "{name}");
    }
}

/// The atmospheric environment's fog, worked out from the fixture's numbers.
fn expected_atmospheric(fx: &Value, eye: &Eye, rain_fog: f32) -> (f32, f32, f32, f32) {
    let a = &fx["fog"]["atmospheric"];
    let rd = &fx["fog"]["render_distance"];
    let d = atmosphere::dimension(eye.dimension);
    let start = d.fog_start + f(&a["rain_start_offset"]) * rain_fog;
    let end = d.fog_end.min(f(&a["rain_min_end"])).max(d.fog_end + f(&a["rain_end_offset"]) * rain_fog);
    let span = (eye.render_distance / f(&rd["span_divisor"])).clamp(f(&rd["span_min"]), f(&rd["span_max"]));
    (start, end, eye.render_distance - span, eye.render_distance)
}

#[test]
fn each_fog_environment_matches_the_client() {
    let fx = fixture();
    let fog = &fx["fog"];
    let order: Vec<&str> = fog["environment_order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        order,
        [
            "LavaFogEnvironment",
            "PowderedSnowFogEnvironment",
            "BlindnessFogEnvironment",
            "DarknessFogEnvironment",
            "WaterFogEnvironment",
            "AtmosphericFogEnvironment"
        ]
    );
    let base = plains_noon();
    let check = |label: &str, eye: Eye, rain: f32, want: (f32, f32, f32, f32)| {
        let got = atmosphere::fog(&eye, RainFog(rain));
        assert!(
            close(got.environmental_start, want.0)
                && close(got.environmental_end, want.1)
                && close(got.sky_end, want.2)
                && close(got.cloud_end, want.3),
            "{label}: {got:?} vs {want:?}"
        );
        let (_, _, rd_start, rd_end) = expected_atmospheric(&fx, &eye, rain);
        assert!(close(got.render_distance_start, rd_start) && close(got.render_distance_end, rd_end), "{label}");
    };
    // Atmospheric: clear, rain under open sky, every render distance.
    for (rd, rain) in [(256.0, 0.0), (64.0, 0.0), (1024.0, 1.0), (256.0, 0.5), (128.0, 1.0)] {
        let eye = Eye { render_distance: rd, ..base };
        let (s, e, _, _) = expected_atmospheric(&fx, &eye, rain);
        let defaults = &fx["attribute_defaults"];
        let sky = rd.min(f(&defaults["sky_fog_end_distance"]));
        check(
            &format!("atmospheric rd {rd} rain {rain}"),
            eye,
            rain,
            (s, e, sky, f(&defaults["cloud_fog_end_distance"])),
        );
    }
    // The Cloud Distance option caps the cloud fog.
    let eye = Eye { cloud_range: 12, ..base };
    check("cloud distance 12", eye, 0.0, (0.0, 1024.0, 256.0, 12.0 * 16.0));
    // The Nether's 10..96 and the End's defaults.
    let nether = Eye { dimension: Dimension::Nether, ..base };
    check("nether", nether, 0.0, (10.0, 96.0, 256.0, 2048.0));
    let end = Eye { dimension: Dimension::End, ..base };
    check("end", end, 0.0, (0.0, 1024.0, 256.0, 2048.0));
    // Lava: normal, fire resistance, spectator.
    let lava = &fog["lava"];
    let eye = Eye { medium: Medium::Lava, ..base };
    let (s, e) = (f(&lava["start"]), f(&lava["end"]));
    check("lava", eye, 0.0, (s, e, e, e));
    let (s, e) = (f(&lava["fire_resistance_start"]), f(&lava["fire_resistance_end"]));
    check("lava, fire resistance", Eye { fire_resistance: true, ..eye }, 0.0, (s, e, e, e));
    let e = 256.0 * f(&lava["spectator_end_fraction"]);
    check("lava, spectator", Eye { spectator: true, ..eye }, 0.0, (f(&lava["spectator_start"]), e, e, e));
    // Lava keeps its fog under blindness.
    let e = f(&lava["end"]);
    check("lava, blind", Eye { blindness: Some(100), ..eye }, 0.0, (f(&lava["start"]), e, e, e));
    // Powder snow.
    let snow = &fog["powder_snow"];
    let e = f(&snow["end"]);
    check("powder snow", Eye { medium: Medium::PowderSnow, ..base }, 0.0, (f(&snow["start"]), e, e, e));
    // Blindness: fading in from the render distance over its last second, and infinite.
    let blind = &fog["blindness"];
    for (ticks, want) in [(200u32, f(&blind["distance"])), (10, 256.0 + (5.0 - 256.0) * 0.5), (u32::MAX, 5.0)] {
        let sky = want * f(&blind["sky_cloud_fraction"]);
        let eye = Eye { blindness: Some(ticks), ..base };
        check(&format!("blind {ticks}"), eye, 0.0, (want * f(&blind["start_fraction"]), want, sky, sky));
        // Blindness wins over water.
        check(
            &format!("blind {ticks} in water"),
            Eye { medium: Medium::Water, ..eye },
            0.0,
            (want * 0.25, want, sky, sky),
        );
    }
    // Darkness.
    let dark = &fog["darkness"];
    for blend in [1.0f32, 0.5] {
        let want = 256.0 + (f(&dark["distance"]) - 256.0) * blend;
        let eye = Eye { darkness: Some(blend), ..base };
        check(&format!("darkness {blend}"), eye, 0.0, (want * f(&dark["start_fraction"]), want, want, want));
    }
    // Water, with water vision and a swamp's shorter fog.
    let water_end = f(&fx["attribute_defaults"]["water_fog_end_distance"]);
    for vision in [0.0f32, 0.5, 1.0] {
        let eye = Eye { medium: Medium::Water, water_vision: vision, ..base };
        let e = water_end * vision.max(f(&fog["water"]["vision_min_end_fraction"]));
        check(&format!("water vision {vision}"), eye, 0.0, (-8.0, e, e, e));
    }
    let swamp = Local::resolve(atmosphere::dimension(Dimension::Overworld), atmosphere::overworld_biome(Biome::Swamp));
    let eye = Eye { medium: Medium::Water, water_vision: 1.0, local: swamp, ..base };
    check("swamp water", eye, 0.0, (-8.0, 96.0 * 0.85, 96.0 * 0.85, 96.0 * 0.85));
}

#[test]
fn fog_colours_follow_the_client_layers() {
    let fx = fixture();
    let base = plains_noon();
    let sky = atmosphere::rgb(0x78A7FF);
    let fog = atmosphere::rgb(0xC0D8FF);
    // Noon, 16 chunks: 1 - lerp(16 / 32, 0.25, 1) ^ 0.25 of the sky.
    let a = &fx["fog"]["atmospheric"];
    let mix = 1.0 - (f(&a["sky_mix_min"]) + (1.0 - f(&a["sky_mix_min"])) * 0.5).powf(f(&a["sky_mix_power"]));
    let want: [f32; 3] = std::array::from_fn(|i| fog[i] + (sky[i] - fog[i]) * mix);
    let got = atmosphere::fog(&base, RainFog(0.0));
    assert!(close3(got.color, want), "{:?} vs {want:?}", got.color);
    assert!(close3(got.sky, sky));
    // Midnight: the fog is multiplied by #0f0f16, the sky by black.
    let night = atmosphere::fog(&Eye { day_ticks: 18000.0, ..base }, RainFog(0.0));
    let dark = atmosphere::rgb(0x0F0F16);
    let want: [f32; 3] = std::array::from_fn(|i| fog[i] * dark[i] * (1.0 - mix));
    assert!(close3(night.color, want) && night.sky == [0.0; 3], "{:?}", night.color);
    // Full rain: the fog colour times (0.5, 0.5, 0.6), the sky blended to grey.
    let w = &fx["weather"]["rain"];
    let eye = Eye { weather: Weather { rain: 1.0, thunder: 0.0 }, ..base };
    let rainy = atmosphere::fog(&eye, RainFog(1.0));
    let m = &w["fog_color_multiply"];
    let grey = (sky[0] * 0.3 + sky[1] * 0.59 + sky[2] * 0.11) * f(&w["sky_color_blend_to_gray"]["brightness"]);
    let factor = f(&w["sky_color_blend_to_gray"]["factor"]);
    let rain_sky: [f32; 3] = std::array::from_fn(|i| sky[i] + (grey - sky[i]) * factor);
    assert!(close3(rainy.sky, rain_sky), "{:?} vs {rain_sky:?}", rainy.sky);
    let dark_sky = [rain_sky[0] * 0.5, rain_sky[1] * 0.5, rain_sky[2] * 0.6];
    let rain_fog = [fog[0] * f(&m[0]), fog[1] * f(&m[1]), fog[2] * f(&m[2])];
    let want: [f32; 3] = std::array::from_fn(|i| rain_fog[i] + (dark_sky[i] - rain_fog[i]) * mix);
    assert!(close3(rainy.color, want), "{:?} vs {want:?}", rainy.color);
    // A lightning flash lerps the sky 0.22 toward (0.8, 0.8, 1).
    let flash = &fx["sky_flash"];
    let lit = atmosphere::fog(&Eye { sky_flash: true, ..base }, RainFog(0.0));
    let t = f(&flash["sky_color_lerp"]);
    let want: [f32; 3] = std::array::from_fn(|i| sky[i] + (f(&flash["flash_color"][i]) - sky[i]) * t);
    assert!(close3(lit.sky, want));
    assert_eq!(atmosphere::sky_light(&Eye { sky_flash: true, day_ticks: 18000.0, ..base }).0, 1.0);
    // Under water: the biome's water fog colour, brightened by water vision.
    let water = atmosphere::fog(&Eye { medium: Medium::Water, ..base }, RainFog(0.0));
    assert!(close3(water.color, atmosphere::rgb(0x050533)));
    let adapted = atmosphere::fog(&Eye { medium: Medium::Water, water_vision: 1.0, ..base }, RainFog(0.0));
    let c = atmosphere::rgb(0x050533);
    assert!(close3(adapted.color, c.map(|v| v / c[2])));
    // Lava and powder snow have fixed colours.
    let lava = atmosphere::fog(&Eye { medium: Medium::Lava, ..base }, RainFog(0.0));
    assert_eq!(lava.color, atmosphere::rgb(hex(&fx["fog"]["lava"]["color"])));
    let snow = atmosphere::fog(&Eye { medium: Medium::PowderSnow, ..base }, RainFog(0.0));
    assert_eq!(snow.color, atmosphere::rgb(hex(&fx["fog"]["powder_snow"]["color"])));
    // Full blindness blacks the fog out; the Nether's colour is its biome's.
    let blind = atmosphere::fog(&Eye { blindness: Some(100), ..base }, RainFog(0.0));
    assert_eq!(blind.color, [0.0; 3]);
    let wastes =
        Local::resolve(atmosphere::dimension(Dimension::Nether), atmosphere::nether_biome(NetherBiome::NetherWastes));
    let nether = atmosphere::fog(&Eye { dimension: Dimension::Nether, local: wastes, ..base }, RainFog(0.0));
    let haze = atmosphere::rgb(0x330808);
    let sky_mix: [f32; 3] = std::array::from_fn(|i| haze[i] * (1.0 - mix));
    assert!(close3(nether.color, sky_mix), "{:?}", nether.color);
}

#[test]
fn rain_fog_follows_the_client_rate() {
    let fx = fixture();
    let a = &fx["fog"]["atmospheric"];
    let eye = Eye { weather: Weather { rain: 1.0, thunder: 0.0 }, ..plains_noon() };
    let mut state = RainFog::default();
    state.update(&eye, 1.0);
    assert!(close(state.0, f(&a["rain_fog_rate_per_tick"])));
    // No precipitation halves the target; sky light 8 and below removes it.
    let mut dry = RainFog::default();
    for _ in 0..500 {
        dry.update(&Eye { precipitation: false, ..eye }, 1.0);
    }
    assert!(close(dry.0, f(&a["no_precipitation_factor"])));
    let mut cave = RainFog::default();
    cave.update(&Eye { sky_light: f(&a["rain_sky_light_min"]) as u8, ..eye }, 1.0);
    assert_eq!(cave.0, 0.0);
}

#[test]
fn water_vision_follows_the_client_curve() {
    let fx = fixture();
    let v = &fx["fog"]["water_vision"];
    assert_eq!(atmosphere::water_vision(0), 0.0);
    assert!(close(atmosphere::water_vision(100), f(&v["early_weight"])));
    assert!(close(atmosphere::water_vision(350), 0.6 + 0.4 * 0.5));
    assert_eq!(atmosphere::water_vision(f(&v["max_ticks"]) as u32), 1.0);
    assert_eq!(atmosphere::water_vision_tick(599, true, false), 600);
    assert_eq!(atmosphere::water_vision_tick(600, true, false), 600);
    assert_eq!(atmosphere::water_vision_tick(5, false, false), 0);
    assert_eq!(atmosphere::water_vision_tick(0, true, true), 10);
}

#[test]
fn cloud_geometry_follows_the_client_rules() {
    let fx = fixture();
    let c = &fx["clouds"];
    assert_eq!(clouds::CELL, f(&c["cell_size_blocks"]));
    assert_eq!(clouds::THICKNESS, f(&c["thickness_blocks"]));
    assert!(close(clouds::DRIFT_PER_TICK as f32, f(&c["drift_blocks_per_tick"])));
    assert!(close(clouds::Z_OFFSET as f32, f(&c["z_offset_blocks"])));
    let shades = &fx["shaders"]["cloud_face_shades"];
    for (i, dir) in ["down", "up", "north", "south", "west", "east"].iter().enumerate() {
        assert_eq!(clouds::SHADES[i], f(&shades[dir]), "{dir}");
    }
    assert_eq!(clouds::radius_cells(128), 171);
    assert_eq!(clouds::radius_cells(2), 3);

    let pattern = Pattern::generate();
    // Below the layer: bottoms, no tops; above: tops, no bottoms; inside: both.
    let below = View::new(5.0, 100.0, 5.0, 0.0, atmosphere::CLOUD_HEIGHT);
    assert_eq!(below.relative, Relative::Below);
    let above = View::new(5.0, 300.0, 5.0, 0.0, atmosphere::CLOUD_HEIGHT);
    assert_eq!(above.relative, Relative::Above);
    let inside = View::new(5.0, 194.0, 5.0, 0.0, atmosphere::CLOUD_HEIGHT);
    assert_eq!(inside.relative, Relative::Inside);
    let mut faces = Vec::new();
    let dirs = |faces: &[u32]| {
        let mut n = [0usize; 6];
        for &face in faces {
            let (_, _, d, flags) = clouds::unpack(face);
            if flags & clouds::INSIDE == 0 {
                n[d as usize] += 1;
            }
        }
        n
    };
    clouds::build(&pattern, &below, Clouds::Fancy, 20, &mut faces);
    let n = dirs(&faces);
    assert!(n[clouds::UP as usize] == 0 && n[clouds::DOWN as usize] > 0);
    clouds::build(&pattern, &above, Clouds::Fancy, 20, &mut faces);
    let n = dirs(&faces);
    assert!(n[clouds::DOWN as usize] == 0 && n[clouds::UP as usize] > 0);
    // Side faces only face the camera: a north face lies south of the camera's cell.
    for &face in &faces {
        let (x, z, d, flags) = clouds::unpack(face);
        if flags & clouds::INSIDE != 0 {
            assert!(x.abs() <= 1 && z.abs() <= 1);
            continue;
        }
        match d {
            clouds::NORTH => assert!(z > 0),
            clouds::SOUTH => assert!(z < 0),
            clouds::WEST => assert!(x > 0),
            clouds::EAST => assert!(x < 0),
            _ => {}
        }
        assert!(x * x + z * z <= 400, "inside the radius");
    }
    // Rings run outward, so faces never get nearer.
    let ring = |face: &u32| {
        let (x, z, _, _) = clouds::unpack(*face);
        x.abs() + z.abs()
    };
    assert!(faces.windows(2).all(|w| ring(&w[0]) <= ring(&w[1])));
    // Fast: one top-coloured downward face per filled cell, nothing for Off.
    clouds::build(&pattern, &below, Clouds::Fast, 20, &mut faces);
    let filled = faces.len();
    assert!(faces.iter().all(|&face| {
        let (_, _, d, flags) = clouds::unpack(face);
        d == clouds::DOWN && flags == clouds::TOP_COLOUR
    }));
    let cells = (-20i32..=20)
        .flat_map(|x| (-20i32..=20).map(move |z| (x, z)))
        .filter(|&(x, z)| x * x + z * z <= 400 && pattern.filled(below.cell.0 + x, below.cell.1 + z))
        .count();
    assert_eq!(filled, cells);
    clouds::build(&pattern, &below, Clouds::Off, 20, &mut faces);
    assert!(faces.is_empty());
    // The layer drifts east 0.03 blocks a tick: 400 ticks move it one cell.
    let later = View::new(5.0, 100.0, 5.0, 400.0, atmosphere::CLOUD_HEIGHT);
    assert_eq!(later.cell.0, below.cell.0 + 1);
    // Clouds show only where the dimension has a cloud colour.
    let base = plains_noon();
    assert!(atmosphere::cloud_color(&base)[3] > 0.0);
    assert_eq!(atmosphere::cloud_color(&Eye { dimension: Dimension::Nether, ..base })[3], 0.0);
    assert_eq!(atmosphere::cloud_color(&Eye { dimension: Dimension::End, ..base })[3], 0.0);
}

#[test]
fn options_match_the_client() {
    let fx = fixture();
    let o = &fx["options"];
    assert_eq!(o["clouds"]["label"], "Clouds");
    let values = o["clouds"]["values"].as_array().unwrap();
    for (c, v) in Clouds::ALL.iter().zip(values) {
        assert_eq!(c.caption(), v["caption"].as_str().unwrap());
        assert_eq!(c.saved(), v["saved"].as_str().unwrap());
    }
    assert_eq!(o["clouds"]["default"], "FANCY");
    assert_eq!(Clouds::default(), Clouds::Fancy);
    let range = &o["cloud_distance"];
    assert_eq!(range["label"], "Cloud Distance");
    assert_eq!(atmosphere::CLOUD_RANGE, (range["min"].as_u64().unwrap() as u32, range["max"].as_u64().unwrap() as u32));
    assert_eq!(atmosphere::DEFAULT_CLOUD_RANGE, range["default"].as_u64().unwrap() as u32);
    assert_eq!(o["hide_sky_flashes"]["label"], "Hide Sky Flashes");
    assert_eq!(o["hide_sky_flashes"]["default"], false);
    assert_eq!(atmosphere::SKY_FLASH_TICKS, fx["sky_flash"]["ticks"].as_u64().unwrap() as u32);
}

/// The atmosphere is presentation only: gameplay light (Java's
/// `sky_light_level` attribute, which spawning and detection read through
/// `SpawnLight`) comes from its own day and weather table, and nothing in
/// the visual model feeds it. Pin both at the same moments to show the
/// visual sky light factor moving while gameplay light stays Java's.
#[test]
fn gameplay_light_does_not_follow_the_visual_model() {
    let base = plains_noon();
    for (ticks, rain, thunder) in [(6000u32, 0.0f32, 0.0f32), (18000, 0.0, 0.0), (6000, 1.0, 1.0), (13000, 1.0, 0.0)] {
        let spawn = SpawnLight::at(ticks, rain, thunder);
        for flash in [false, true] {
            let eye = Eye { day_ticks: f64::from(ticks), weather: Weather { rain, thunder }, sky_flash: flash, ..base };
            let _ = atmosphere::fog(&eye, RainFog(rain));
            let _ = atmosphere::sky_light(&eye);
            assert_eq!(SpawnLight::at(ticks, rain, thunder), spawn, "{ticks} {rain} {thunder} {flash}");
        }
    }
    assert_eq!(SpawnLight::at(6000, 0.0, 0.0), SpawnLight::DAY);
    assert_eq!(SpawnLight::at(18000, 0.0, 0.0), SpawnLight::NIGHT);
    // A lightning flash lights the rendered sky fully at midnight; gameplay stays dark.
    let flash = Eye { day_ticks: 18000.0, sky_flash: true, ..base };
    assert_eq!(atmosphere::sky_light(&flash).0, 1.0);
    assert_eq!(SpawnLight::at(18000, 0.0, 0.0).sky_darken, SpawnLight::NIGHT.sky_darken);
}
