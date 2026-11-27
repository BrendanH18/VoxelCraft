//! World startup and restoration; the application only coordinates screens.
use super::*;
impl Game {
    /// Loads the world saved in `saves_dir/world_dir`, or creates it (also
    /// when `new` describes a world made on the title screen).
    pub(super) fn start(
        shell: Shell,
        args: &Args,
        saves_dir: &std::path::Path,
        world_dir: &str,
        new: Option<title::NewWorld>,
    ) -> Game {
        let Shell { renderer, audio, settings, settings_path, updates, gilrs } = shell;
        // v0.18 Track 4: saved hotbars live beside `options.txt`, like Java's `hotbar.nbt`.
        let hotbars_path = settings_path.as_ref().map(|_| saves_dir.join("hotbars.txt"));
        let storage = Storage::new(saves_dir.join(world_dir));
        let existing = if args.join.is_some() || args.new_world || new.is_some() || !storage.exists() {
            None
        } else {
            match storage.load_level() {
                Ok(level) => Some(level),
                Err(e) => {
                    // `App::play` refuses unreadable levels; this is a file changed since.
                    log::error!("failed to load level, starting fresh: {e}");
                    None
                }
            }
        };
        // `--new` replaces the save, so the old world's Nether must not carry
        // over into the new one. Scripted runs never write saves.
        if args.join.is_none() && args.new_world && settings_path.is_some() {
            for dimension in [Dimension::Nether, Dimension::End] {
                let nether = dimension::storage_for(&storage, dimension);
                if nether.exists()
                    && let Err(e) = std::fs::remove_dir_all(nether.dir())
                {
                    log::error!("failed to remove the old dimension save {}: {e}", nether.dir().display());
                }
            }
            // v1.0 Track 3b: the old world's overworld records too.
            let regions = storage.dir().join(crate::world::region::REGION_DIR);
            if regions.is_dir()
                && let Err(e) = std::fs::remove_dir_all(&regions)
            {
                log::error!("failed to remove the old region files {}: {e}", regions.display());
            }
        }
        let seed =
            existing.as_ref().map(|l| l.seed).or(new.as_ref().and_then(|n| n.seed)).or(args.seed).unwrap_or_else(
                || {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(1)
                },
            );
        let root_props = existing.as_ref().map(|l| l.props.clone()).unwrap_or_default();
        let saved_dimension = root_props.get("dimension").and_then(|d| Dimension::from_name(d)).unwrap_or_default();
        let dimension = args.dimension.unwrap_or(saved_dimension);
        let overworld_props = dimension::dimension_props(&root_props);
        let (saved, dimension_props, load_notice) = if existing.is_some() {
            dimension::load_dimension(&storage, dimension, &overworld_props)
        } else {
            Default::default()
        };
        log::info!("world '{world_dir}' seed {seed}, {} modified chunks in the {}", saved.len(), dimension.name());

        let mode = new
            .as_ref()
            .map(|n| n.mode)
            .or(args.mode)
            .or_else(|| existing.as_ref().and_then(|l| l.props.get("mode")).and_then(|m| GameMode::from_name(m)))
            .unwrap_or_default();
        let hardcore = starting_hardcore(new.as_ref(), existing.is_some(), args, &root_props);
        let difficulty = if hardcore {
            Difficulty::Hard
        } else {
            new.as_ref()
                .map(|n| n.difficulty)
                .or_else(|| root_props.get("difficulty").and_then(|d| Difficulty::from_name(d)))
                .unwrap_or_default()
        };
        let gamerules = starting_rules(root_props.get("gamerules").map(String::as_str), args);
        let inventory = existing
            .as_ref()
            .and_then(|l| l.props.get("inventory"))
            .and_then(|s| Inventory::deserialize(s))
            .unwrap_or_else(|| match mode {
                GameMode::Creative => Inventory::with_hotbar(&CREATIVE_HOTBAR),
                GameMode::Survival | GameMode::Adventure | GameMode::Spectator => Inventory::default(),
            });

        let mut inventory = inventory;
        for &(item, count) in &args.give {
            inventory.add(item, count);
        }
        for &item in &args.wear {
            if let Some((piece, _)) = item.as_armor() {
                inventory.armor[piece as usize] = Some(Stack::new(item, 1));
            }
        }
        if !args.dyes.is_empty() {
            use voxelcraft::inventory::components::apply_dyes;
            for slot in inventory.armor.iter_mut().chain(&mut inventory.slots[..1]) {
                if let Some(dyed) = slot.and_then(|s| apply_dyes(s, &args.dyes)) {
                    *slot = Some(dyed);
                }
            }
        }
        for &(e, level) in &args.enchants {
            match crate::enchant::command(inventory.slots[0], e, level) {
                Ok(stack) => inventory.slots[0] = Some(stack),
                Err(err) => log::warn!("--enchant: {err}"),
            }
        }

        let prop = |k: &str| existing.as_ref().and_then(|l| l.props.get(k));
        let mut vitals = Vitals::restore(
            prop("health").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_HEALTH),
            prop("air").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_AIR),
            prop("death").cloned(),
        );
        if let Some(h) = args.health {
            vitals = Vitals::restore(h, vitals.air, None);
        }
        if let Some(a) = args.air {
            vitals.air = a.clamp(0.0, survival::MAX_AIR);
        }
        if let Some(text) = prop("hunger") {
            let n: Vec<f32> = text.split(',').filter_map(|v| v.parse().ok()).collect();
            if let [food, saturation, exhaustion] = n[..] {
                vitals.hunger = survival::Hunger::restore(food, saturation, exhaustion);
            }
        }
        if let Some(f) = args.food {
            vitals.hunger = survival::Hunger::restore(f, 0.0, 0.0);
        }
        if let Some(xp) = prop("xp").and_then(|t| crate::simulation::experience::Experience::parse(t)) {
            vitals.xp = xp;
        }
        vitals.time_since_rest =
            prop("time_since_rest").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0).min(i32::MAX as u32);
        vitals.ticks_frozen = prop("ticks_frozen").and_then(|s| s.parse::<u16>().ok()).unwrap_or(0).min(140);
        if let Some(v) = prop("warden_spawn_tracker").and_then(|t| serde_json::from_str(t).ok()) {
            vitals.warden = crate::world::sculk::WardenSpawnTracker::load(&v);
        }
        if let Some(text) = prop("effects") {
            vitals.effects = crate::simulation::effects::Effects::deserialize(text);
            if let Some(h) = prop("health").and_then(|s| s.parse().ok()) {
                vitals.restore_boosted_health(h);
            }
        }
        for &instance in &args.effects {
            let (damage, _) = vitals.give_effect(instance);
            vitals.damage(damage, voxelcraft::simulation::damage::Cause::MAGIC, mode.invulnerable());
        }
        if let Some(level) = args.xp {
            let total = (0..level.min(1000)).map(crate::simulation::experience::points_to_next).sum();
            vitals.xp = crate::simulation::experience::Experience::restore(level, 0, total);
        }

        let generation = world_generation(existing.as_ref());
        let generator = Arc::new(Generator::with_generation(seed, dimension, generation));
        let overworld = Generator::with_generation(seed, Dimension::Overworld, generation);
        let mut player = Player::new(overworld.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        if let Some((pos, yaw, pitch)) = existing.as_ref().and_then(|l| l.player) {
            player.pos = pos;
            player.yaw = yaw;
            player.pitch = pitch;
        }
        // `--dimension` into the other world: arrive by portal, as if the
        // player had walked through one where they stood.
        let arrival = (dimension != saved_dimension && args.pose.is_none()).then(|| {
            if dimension == Dimension::End {
                return dimension::Arrival::EndSpawn;
            }
            let scale = if dimension == Dimension::Nether { 1.0 / 8.0 } else { 8.0 };
            let p = (player.pos * DVec3::new(scale, 0.0, scale)).floor().as_ivec3();
            dimension::Arrival::Portal(p.with_y(if dimension == Dimension::Nether { 64 } else { 100 }), true)
        });
        if let Some([x, y, z, yaw, pitch]) = args.pose {
            player.pos = DVec3::new(x, y, z);
            player.yaw = (yaw as f32).to_radians();
            player.pitch = (pitch as f32).to_radians();
            player.flying = true;
        }
        player.can_fly = mode.can_fly();
        player.noclip = mode == GameMode::Spectator;
        if mode == GameMode::Spectator {
            player.flying = true;
        }
        let mut world = World::new(generator, saved, settings.render_distance);
        world.set_simulation_distance(settings.simulation_distance);
        // v1.0 Track 3b: unloaded chunks live in region files. A LAN client
        // has none; scripted runs read them but never write.
        let load_notice = if args.join.is_none() {
            let writable = settings_path.is_some();
            let region = dimension::attach_regions(&mut world, &storage, dimension, &dimension_props, writable);
            load_notice.into_iter().chain(region).reduce(|a, b| format!("{a} {b}"))
        } else {
            load_notice
        };
        world.set_tile_drops(gamerules.bool("block_drops"));
        log::info!("{} worker threads", world.worker_threads());

        let now = Instant::now();
        let previous_eye = player.eye();
        let mut agents = agents::Agents { cheats: args.agent_cheats, ..Default::default() };
        if let Some(address) = args.agent_listen {
            agents.host =
                Some(voxelcraft::control::Host::bind(address, args.agent_token.clone()).unwrap_or_else(|e| {
                    eprintln!("agent host: {e}");
                    std::process::exit(1);
                }));
            eprintln!("Agent host listening on {}", agents.host.as_ref().unwrap().address);
        }
        if let Some(text) = root_props.get("agents") {
            agents.restore(text, dimension.name(), player.pos);
        }
        if let Some(text) = root_props.get("pad_beds") {
            agents.restore_pad_beds(text);
        }
        let mut session = voxelcraft::session::PlayerSession::from_parts(
            crate::entity::PlayerId::HOST,
            player,
            inventory,
            vitals,
            mode,
        );
        session.spawn_bed = existing.as_ref().and_then(|l| l.props.get("bed")).and_then(|t| {
            let v: Vec<i32> = t.split(',').filter_map(|s| s.parse().ok()).collect();
            (v.len() == 3).then(|| glam::IVec3::new(v[0], v[1], v[2]))
        });
        session.spawn_point = existing.as_ref().and_then(|l| l.props.get("spawn_point")).and_then(|text| {
            let n: Vec<i32> = text.split(',').filter_map(|value| value.parse().ok()).collect();
            (n.len() == 3).then(|| IVec3::new(n[0], n[1], n[2]))
        });
        if let Some(text) = existing.as_ref().and_then(|l| l.props.get("last_death")) {
            session.load_last_death(text);
        }
        if let Some(text) = existing.as_ref().and_then(|l| l.props.get("raid_omen_position")) {
            session.load_raid_omen(text);
        }
        if let Some(text) = existing.as_ref().and_then(|l| l.props.get("stats")) {
            session.stats = voxelcraft::session::raid::Stats::parse(text);
        }
        // v0.18 Track 5: a save from before the recipe book starts from Java's
        // initial book; the first tick checks the loaded inventory.
        if let Some(text) = existing.as_ref().and_then(|l| l.props.get("recipe_book")) {
            session.recipe_book = crate::crafting::book::RecipeBook::deserialize(text);
        }
        session.spawn_anchor = existing.as_ref().and_then(|l| l.props.get("spawn_anchor")).and_then(|text| {
            let n: Vec<i32> = text.split(',').filter_map(|value| value.parse().ok()).collect();
            (n.len() == 3).then(|| IVec3::new(n[0], n[1], n[2]))
        });
        session.ender = existing
            .as_ref()
            .and_then(|l| l.props.get("ender_chest"))
            .and_then(|text| crate::world::chest::Chest::deserialize(text))
            .unwrap_or_default();
        let mut game = Game {
            session,
            session_lock: None,
            agents,
            lan: lan::Lan::default(),
            split: split::Split { follow: args.split_screen.clone(), side_by_side: args.split_side },
            // Screenshot runs never read controllers.
            virtual_pad: args.pad_player.clone(),
            pads: {
                let mut pads = gamepad::Pads::new(gilrs);
                let prop = |k: &str| root_props.get(k).map_or("", String::as_str);
                pads.restore(prop("pads"));
                pads
            },
            search: search::Search {
                query: args.inventory_search.clone().unwrap_or_default(),
                focused: args.open_inventory && args.screenshot.is_none(),
                ..Default::default()
            },
            console: console::Console { open: args.open_console, input: "/".into(), ..Default::default() },
            renderer,
            world,
            storage,
            keys: FxHashSet::default(),
            sneak_toggled: false,
            sprint_toggled: false,
            respawn_due: false,
            scroll: 0.0,
            frame_limit: Default::default(),
            distortion: Default::default(),
            modifiers: Default::default(),
            mouse_grabbed: false,
            jump_pressed: false,
            difficulty,
            hardcore,
            difficulty_locked: root_props.get("difficulty_locked").is_some_and(|v| v == "true"),
            gamerules,
            inventory_open: args.open_inventory,
            craft: crate::crafting::Grid::new(2),
            recipe_book: Default::default(),
            creative: Default::default(),
            hotbars: hotbars_path
                .as_deref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| voxelcraft::creative::Hotbars::deserialize(&t))
                .unwrap_or_default(),
            hotbars_path,
            work_scroll: 0,
            cursor_px: (0.0, 0.0),
            body: Default::default(),
            show_hud: !args.no_hud,
            captions: Vec::new(),
            hand: Default::default(),
            camera: args.camera,
            scripted_ride: args.ride.clone(),
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            clock: Default::default(),
            previous_eye,
            rendered_eye: previous_eye,
            atmosphere_views: Default::default(),
            day_time: args
                .time
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("time")).and_then(|t| t.parse().ok()))
                .unwrap_or(0.08),
            day_count: root_props.get("day_count").and_then(|value| value.parse().ok()).unwrap_or(0),
            credits_seen: root_props.get("credits_seen").is_some_and(|v| v == "1"),
            credits: None,
            world_spawn: root_props
                .get("world_spawn")
                .and_then(|text| {
                    let n: Vec<i32> = text.split(',').filter_map(|value| value.parse().ok()).collect();
                    (n.len() == 3).then(|| IVec3::new(n[0], n[1], n[2]))
                })
                .unwrap_or_else(|| overworld.find_spawn()),
            weather: starting_weather(
                seed,
                existing.as_ref().and_then(|l| l.props.get("weather")).map(String::as_str),
                args,
            ),
            weather_verts: Vec::new(),
            started: now,
            animation_time: args.animation_time,
            dial_wobble: Default::default(),
            last_save: now,
            stats_since: now,
            frames: 0,
            frame_time_sum: 0.0,
            fps: 0.0,
            cpu_ms: 0.0,
            show_debug: args.debug_overlay,
            popup: (String::new(), now - Duration::from_secs(10)),
            save_error: None,
            screenshot: args.screenshot.clone(),
            // v0.18 Track 7: a world this game hosts takes its icon; scripted
            // runs only when asked (`--world-icon`), LAN clients never.
            world_icon: (args.join.is_none() && (settings_path.is_some() || args.world_icon))
                .then(|| world_icon::WorldIcon::new(saves_dir.join(world_dir))),
            panorama_out: args.panorama.clone(),
            panorama_face: None,
            screenshot_state: 0,
            screenshot_settled: None,
            open_trading: args.open_trading,
            place: args.place.clone(),
            signs: args.signs.clone(),
            carts: args.carts.clone(),
            cart_sound_tick: 0,
            decor_scene: args.decor.clone(),
            brush_scene: args.brush.clone(),
            open_block: args.open_block,
            drop: args.drop.clone(),
            orbs: args.orbs.clone(),
            placed: false,
            bench_render: args.bench_render.then(Vec::new),
            benchmark: benchmark::Run::from_args(args),
            frame_started: None,
            audio,
            mobs: mobs::Mobs::new(seed, args.spawn.clone(), args.wait)
                .holding(args.spawn_held.clone())
                .still(args.still_mobs),
            particles: crate::particles::System::new(seed),
            menu: menu::startup_menu(args.open_menu.as_deref()),
            menu_action: None,
            death: Default::default(),
            anvil_name: Default::default(),
            death_respawn_sent: None,
            had_focus: false,
            settings,
            settings_path,
            updates,
            dimension,
            overworld_props: if dimension != Dimension::Overworld { overworld_props } else { Default::default() },
            arrival,
            portal_time: 0.0,
            portal_locked: false,
            portal_cooldown: 0.0,
            terrain_version: existing
                .as_ref()
                .map_or(2, |l| l.props.get("terrain").and_then(|v| v.parse().ok()).unwrap_or(1)),
            world_name: new
                .map(|n| n.name)
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("name")).cloned())
                .unwrap_or_else(|| world_dir.to_string()),
        };
        if game.console.open {
            game.set_grab(false);
        }
        game.renderer.force_offscreen =
            game.bench_render.is_some() || (game.benchmark.is_some() && args.benchmark_offscreen);
        game.world.loot_sequences = root_props
            .get("loot_sequences")
            .and_then(|s| crate::world::loot::RandomSequences::load(s))
            .unwrap_or_default();
        game.world.maps =
            root_props.get("maps").and_then(|s| crate::world::maps::MapStorage::load(s)).unwrap_or_default();
        game.restore_dimension(&dimension_props);
        // v0.18 Track 0's capture flags: `--open-recipe-book` opens the
        // crafting book (Track 5), `--creative-tab` selects a creative tab
        // (Track 4; the operator tab isn't shown, so it stays on the default).
        if args.open_recipe_book {
            game.session.recipe_book.set_open(crate::crafting::book::BookType::Crafting, true);
        }
        if let Some(id) = args.creative_tab
            && let Some(tab) = voxelcraft::creative::catalogue().tabs.iter().position(|t| t.id == id)
        {
            game.creative_select(tab);
        }
        if game.session.vitals.is_dead() {
            game.on_death();
        } else if game.screenshot.is_none()
            && game.bench_render.is_none()
            && game.benchmark.is_none()
            && !game.console.open
        {
            game.set_grab(true);
        }
        game.lan.mode = game.session.mode;
        game.lan.cheats = args.lan_cheats;
        game.lan.pvp = args.lan_pvp;
        game.lan.restore_rules(root_props.get("lan_rules").map(String::as_str));
        game.lan.profile = args.profile.clone().unwrap_or_else(|| lan::local_profile(saves_dir));
        if let Some(address) = args.host_lan
            && let Err(e) = game.open_lan(address)
        {
            game.show_popup(&e);
        }
        // v1.0 Track 1: damage the load worked around.
        if let Some(notice) = load_notice {
            game.report_load_damage(&notice);
        }
        game
    }
}

/// Whether the world is Hardcore: chosen on Create New World, saved in the
/// level file, or `--hardcore` for a world this run creates.
fn starting_hardcore(
    new: Option<&title::NewWorld>,
    existing: bool,
    args: &Args,
    root_props: &std::collections::BTreeMap<String, String>,
) -> bool {
    new.is_some_and(|n| n.hardcore)
        || (args.hardcore && !existing)
        || root_props.get("hardcore").is_some_and(|v| v == "true" || v == "1")
}

/// The saved game rules, with `--gamerule` values set over them (parsing
/// already checked each name and value).
fn starting_rules(saved: Option<&str>, args: &Args) -> GameRules {
    let mut rules = saved.map_or_else(GameRules::default, GameRules::deserialize);
    for (name, value) in &args.gamerules {
        if let Err(e) = rules.set(name, value) {
            log::warn!("--gamerule {name}={value}: {e}");
        }
    }
    rules
}

/// The saved weather, or `--weather`'s at full strength from the start.
fn starting_weather(seed: u64, saved: Option<&str>, args: &Args) -> weather::Weather {
    let mut w = weather::Weather::new(seed);
    if let Some(text) = saved {
        w.deserialize(text);
    }
    match args.weather {
        Some(true) if args.thunder => w.set_thunder(true),
        Some(rain) => w.set(rain, true),
        None => {}
    }
    w
}

/// The generator a world uses: new worlds take the latest, saved ones keep
/// the one their `level.txt` records (see [`super::save::record_generation`]).
fn world_generation(existing: Option<&LevelInfo>) -> Generation {
    existing
        .map_or(Generation::LATEST, |level| Generation::from_prop(level.props.get(Generation::KEY).map(String::as_str)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        crate::parse_args_from(list.iter().map(|s| s.to_string())).unwrap()
    }

    #[test]
    fn bench_lengths_must_be_finite() {
        for bad in ["NaN", "inf", "-1"] {
            assert!(crate::parse_args_from(["--bench-seconds", bad].map(String::from)).is_err(), "{bad}");
            assert!(crate::parse_args_from(["--bench-warmup", bad].map(String::from)).is_err(), "{bad}");
        }
        assert_eq!(args(&["--bench-seconds", "30"]).bench_seconds, 30.0);
    }

    #[test]
    fn gui_scale_overrides_the_options_for_the_session() {
        let mut settings = settings::Settings::default();
        assert_eq!(settings.gui_scale, 0, "scripted runs start from Auto");
        super::super::apply_session_overrides(&mut settings, &args(&["--screenshot", "x.png", "--gui-scale", "2"]));
        assert_eq!(settings.gui_scale, 2);
        super::super::apply_session_overrides(&mut settings, &args(&["--screenshot", "x.png", "--gui-scale", "auto"]));
        assert_eq!(settings.gui_scale, 0);
        super::super::apply_session_overrides(&mut settings, &args(&[]));
        assert_eq!(settings.gui_scale, 0, "no flag keeps the option");
        assert!(
            crate::parse_args_from(["--gui-scale", "2"].map(String::from)).is_err(),
            "a normal run keeps its saved GUI Scale"
        );
        // At 1280x720 a requested 4 is capped at what fits, as in the options.
        assert_eq!(crate::render::ui::Ui::gui_scale(1280.0, 720.0, 4), 3.0);
        assert!(crate::parse_args_from(["--gui-scale".to_string(), "huge".to_string()]).is_err());
    }

    #[test]
    fn recipe_book_and_creative_tab_open_their_screens() {
        let book = args(&["--open-recipe-book"]);
        assert!(book.open_recipe_book && book.open_inventory, "the recipe book opens in the inventory");
        let table = args(&["--open-recipe-book", "--open-block", "0,64,0"]);
        assert!(table.open_recipe_book && !table.open_inventory, "or beside the block's screen");

        let tab = args(&["--creative-tab", "minecraft:combat"]);
        assert_eq!(tab.creative_tab, Some("combat"));
        assert_eq!(tab.mode, Some(GameMode::Creative));
        assert!(tab.open_inventory);
        assert!(crate::parse_args_from(["--survival", "--creative-tab", "search"].map(String::from)).is_err());
        assert!(crate::parse_args_from(["--creative-tab", "everything"].map(String::from)).is_err());

        let mut once = args(&["--open-recipe-book", "--creative-tab", "search", "--hardcore"]);
        once.clear_one_shot();
        assert!(!once.open_recipe_book && once.creative_tab.is_none() && !once.hardcore);
    }

    #[test]
    fn hardcore_applies_to_worlds_this_run_creates() {
        let none = std::collections::BTreeMap::new();
        let hard = args(&["--hardcore"]);
        assert!(starting_hardcore(None, false, &hard, &none));
        assert!(!starting_hardcore(None, true, &hard, &none), "a saved world keeps its own setting");
        assert!(!starting_hardcore(None, false, &args(&[]), &none));
        let saved = std::collections::BTreeMap::from([("hardcore".to_string(), "true".to_string())]);
        assert!(starting_hardcore(None, true, &args(&[]), &saved));
    }

    #[test]
    fn gamerules_and_weather_hold_a_capture_steady() {
        let a = args(&["--gamerule", "advance_time=false", "--gamerule", "advance_weather=false"]);
        let rules = starting_rules(None, &a);
        assert!(!rules.bool("advance_time") && !rules.bool("advance_weather"));
        let saved = GameRules::default().serialize();
        assert!(!starting_rules(Some(&saved), &a).bool("advance_time"), "set over the saved rules");
        assert!(starting_rules(None, &args(&[])).bool("advance_time"));
        assert!(crate::parse_args_from(["--gamerule", "advance_time=soon"].map(String::from)).is_err());

        let thunder = starting_weather(1, None, &args(&["--weather", "thunder"]));
        assert!(thunder.raining && thunder.thundering && thunder.is_thundering(), "at full strength at once");
        let rain = starting_weather(1, None, &args(&["--weather", "rain"]));
        assert!(rain.raining && !rain.thundering && rain.strength == 1.0);
        let clear = starting_weather(1, Some("1,100,1,100"), &args(&["--weather", "clear"]));
        assert!(!clear.raining && !clear.thundering && clear.strength == 0.0);
    }

    #[test]
    fn new_worlds_save_and_reload_as_the_latest_generation() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-worldgen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(&dir);

        // A new world: the latest generation (7 since v1.0 gap Track 4), written as `worldgen=7`.
        let generation = world_generation(None);
        assert_eq!(generation, Generation::V7);
        let generator = Generator::with_generation(7, Dimension::Overworld, generation);
        let mut props = std::collections::BTreeMap::new();
        super::super::save::record_generation(&mut props, generator.generation);
        storage.save(&LevelInfo { seed: 7, player: None, props }, &[]).unwrap();
        let text = std::fs::read_to_string(dir.join("level.txt")).unwrap();
        assert!(text.lines().any(|line| line == "worldgen=7"), "{text}");

        // Reloaded, it stays generation 7.
        let saved = storage.load_level().unwrap();
        assert_eq!(world_generation(Some(&saved)), Generation::V7);
        assert_eq!(Generator::with_generation(saved.seed, Dimension::Overworld, Generation::V7).generation.number(), 7);

        // Older worlds keep theirs.
        storage.set_props(&[(Generation::KEY, "6")]).unwrap();
        assert_eq!(world_generation(Some(&storage.load_level().unwrap())), Generation::V6);
        storage.set_props(&[(Generation::KEY, "5")]).unwrap();
        assert_eq!(world_generation(Some(&storage.load_level().unwrap())), Generation::V5);
        storage.set_props(&[(Generation::KEY, "2")]).unwrap();
        assert_eq!(world_generation(Some(&storage.load_level().unwrap())), Generation::V2);
        let mut legacy = storage.load_level().unwrap();
        legacy.props.remove(Generation::KEY);
        assert_eq!(world_generation(Some(&legacy)), Generation::V1, "saved before v0.13");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
