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
        let (saved, dimension_props) = if existing.is_some() {
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
        let hardcore = new.as_ref().is_some_and(|n| n.hardcore)
            || root_props.get("hardcore").is_some_and(|v| v == "true" || v == "1");
        let difficulty = if hardcore {
            Difficulty::Hard
        } else {
            new.as_ref()
                .map(|n| n.difficulty)
                .or_else(|| root_props.get("difficulty").and_then(|d| Difficulty::from_name(d)))
                .unwrap_or_default()
        };
        let gamerules =
            root_props.get("gamerules").map_or_else(GameRules::default, |text| GameRules::deserialize(text));
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
        vitals.ticks_frozen = prop("ticks_frozen").and_then(|s| s.parse::<u16>().ok()).unwrap_or(0).min(140);
        if let Some(text) = prop("effects") {
            vitals.effects = crate::simulation::effects::Effects::deserialize(text);
        }
        for &(effect, secs, amp) in &args.effects {
            let damage = vitals.apply_effect(effect, amp, secs.saturating_mul(20).max(1));
            vitals.damage(damage, survival::CAUSE_MAGIC, mode.invulnerable());
        }
        if let Some(level) = args.xp {
            let total = (0..level.min(1000)).map(crate::simulation::experience::points_to_next).sum();
            vitals.xp = crate::simulation::experience::Experience::restore(level, 0, total);
        }

        let generator = Arc::new(Generator::for_dimension(seed, dimension));
        let mut player = Player::new(Generator::new(seed).find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
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
            dimension::Arrival::Portal(p.with_y(if dimension == Dimension::Nether { 64 } else { 100 }))
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
            recipe_book: recipe_book::RecipeBook::default(),
            creative_scroll: 0,
            cursor_px: (0.0, 0.0),
            body: Default::default(),
            show_hud: !args.no_hud,
            hand: Default::default(),
            camera: args.camera,
            scripted_ride: args.ride.clone(),
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            clock: Default::default(),
            previous_eye,
            rendered_eye: previous_eye,
            nether_fog: Default::default(),
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
                .unwrap_or_else(|| Generator::new(seed).find_spawn()),
            weather: {
                let mut w = weather::Weather::new(seed);
                if let Some(text) = existing.as_ref().and_then(|l| l.props.get("weather")) {
                    w.deserialize(text);
                }
                if let Some(rain) = args.weather {
                    w.set(rain, true);
                }
                w
            },
            weather_verts: Vec::new(),
            started: now,
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
            screenshot_state: 0,
            open_trading: args.open_trading,
            place: args.place.clone(),
            signs: args.signs.clone(),
            carts: args.carts.clone(),
            decor_scene: args.decor.clone(),
            open_block: args.open_block,
            drop: args.drop.clone(),
            orbs: args.orbs.clone(),
            placed: false,
            bench_render: args.bench_render.then(Vec::new),
            benchmark: benchmark::Run::from_args(args),
            frame_started: None,
            audio,
            mobs: mobs::Mobs::new(seed, args.spawn.clone(), args.wait),
            particles: crate::particles::System::new(seed),
            menu: menu::startup_menu(args.open_menu.as_deref()),
            menu_action: None,
            had_focus: false,
            settings,
            settings_path,
            updates,
            dimension,
            overworld_props: if dimension != Dimension::Overworld { overworld_props } else { Default::default() },
            arrival,
            portal_time: 0.0,
            portal_locked: false,
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
        game.renderer.force_offscreen = game.bench_render.is_some();
        game.restore_dimension(&dimension_props);
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
        game
    }
}
