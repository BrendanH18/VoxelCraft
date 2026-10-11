#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod audio;
mod bench;
mod data;
mod render;

use voxelcraft::{
    color, crafting, enchant, entity, inventory, item, mesh, mining, particles, physics, player, simulation, smithing,
    world,
};

use winit::event_loop::{ControlFlow, EventLoop};

pub struct Args {
    pub seed: Option<u64>,
    pub host_lan: Option<std::net::SocketAddr>,
    pub join: Option<String>,
    pub profile: Option<String>,
    pub agent_listen: Option<std::net::SocketAddr>,
    pub agent_token: Option<String>,
    pub agent_cheats: bool,
    pub open_console: bool,
    /// `--split-screen`: agent profiles to follow in extra views.
    pub split_screen: Vec<String>,
    /// `--split-layout side`: two views side by side.
    pub split_side: bool,
    /// `--world`: load this save directly instead of showing the title screen.
    pub world: Option<String>,
    pub data_dir: Option<std::path::PathBuf>,
    /// Overrides the saved option for this session.
    pub render_distance: Option<i32>,
    pub no_vsync: bool,
    pub enhanced_graphics: Option<bool>,
    pub new_world: bool,
    pub bench: bool,
    pub screenshot: Option<String>,
    /// Session-only perspective for screenshot inspection.
    pub camera: voxelcraft::camera::CameraMode,
    pub ride: Option<String>,
    pub bench_render: bool,
    pub debug_overlay: bool,
    pub mode: Option<app::GameMode>,
    pub open_inventory: bool,
    /// Open the targeted villager after its first job claim (screenshots).
    pub open_trading: bool,
    /// Seat a virtual controller player with this screen open (screenshots).
    pub pad_player: Option<String>,
    pub inventory_search: Option<String>,
    /// Start with the pause menu or options screen open (screenshots).
    pub open_menu: Option<String>,
    /// Starting time of day, 0..1 (0 sunrise, 0.25 noon, 0.75 midnight).
    pub time: Option<f64>,
    /// `--weather`: start raining (true) or clear (false).
    pub weather: Option<bool>,
    /// `--dimension`: start in the Overworld, Nether or End.
    pub dimension: Option<world::terrain::Dimension>,
    /// Blocks to set once the world has loaded (debugging/screenshots).
    pub place: Vec<(glam::IVec3, world::block::Block)>,
    /// Opens the container at this block once loaded (screenshots).
    pub open_block: Option<glam::IVec3>,
    /// Debug cart placements on rails once the world loads.
    pub carts: Vec<(entity::minecart::CartKind, glam::IVec3)>,
    /// Mobs to spawn once the world has loaded (y = i32::MIN: surface).
    /// The optional armor material forces a full set on a zombie or skeleton.
    pub spawn: Vec<(entity::MobKind, glam::IVec3, Option<entity::armor::Equipped>)>,
    /// Seconds to keep running after loading before `--screenshot`.
    pub wait: f64,
    /// x,y,z,yaw_deg,pitch_deg
    pub pose: Option<[f64; 5]>,
    /// Starting health / air overrides (debugging/screenshots).
    pub health: Option<f32>,
    pub air: Option<f32>,
    pub food: Option<f32>,
    /// Starting experience level (`--xp`).
    pub xp: Option<u32>,
    /// Status effects to start with: (effect, seconds, amplifier).
    pub effects: Vec<(voxelcraft::simulation::effects::Effect, u32, u8)>,
    /// Experience orbs (points each) spawned in front of the player once loaded.
    pub orbs: Vec<u32>,
    /// Items added to the inventory at startup (debugging/screenshots).
    pub give: Vec<(item::Item, u8)>,
    /// Armor worn at startup (`--wear`).
    pub wear: Vec<item::Item>,
    /// Enchantments put on the first stack given (`--enchant`).
    pub enchants: Vec<(enchant::Enchantment, u8)>,
    /// Items thrown in front of the player once the world has loaded.
    pub drop: Vec<(item::Item, u8)>,
    /// Sound: start muted, master volume 0..1, dump WAVs and exit.
    pub mute: bool,
    /// Start with the HUD hidden (F1), for clean screenshots.
    pub no_hud: bool,
    pub volume: Option<f32>,
    pub export_sounds: bool,
    pub export_music: bool,
}

const USAGE: &str = "\
voxelcraft [options]
  --host-lan <IP:PORT>     open a desktop world to LAN (0.0.0.0:0 chooses a port)
  --join <HOST:PORT>       join a LAN world; never writes its save
  --profile <name>         local LAN identity (restored on reconnect)
  --agent-listen <IP:PORT>  host CLI players (default recommended: 127.0.0.1:4242)
  --agent-token <token>    shared token (required for LAN, at least 16 characters)
  --agent-cheats           permit agents to use give, gamemode, tp and world commands
  --open-console          start with the slash command console open
  --split-screen <names>  follow these agent players (comma-separated, up to 3)
                          in split-screen views; /splitscreen changes it in game
  --split-layout <stacked|side>  two views top/bottom (default) or side by side
  --seed <n>        world seed (new worlds only)
  --world <name>    load or create this save, skipping the title screen
                    (letters, digits, - or _)
  --data-dir <dir>  override the per-user data folder (saves and logs)
  --version         show the game version
  --rd <chunks>     render distance in 32-block chunks (default: 8, or the
                    saved option)
  --new             ignore any existing save and start a fresh world (in
                    --world, default: world)
  --no-vsync        uncapped frame rate
  --graphics <m>   enhanced (default) or classic lighting and water
  --bench           headless terrain generation + meshing benchmark
  --bench-render    load the world, render a 360° sweep offscreen, report frame times
  --creative, --survival  game mode (default: survival, or the saved mode)
  --f3              start with the debug overlay open
  --inventory-search <text>  initial inventory search query
  --open-trading    open a targeted villager once it claims a job (screenshots)
  --open-inventory  start with the inventory screen open (screenshots)
  --pad-player <s>  seat a controller player holding a copy of your
                    inventory, with play, pause, inventory, crafting or
                    palette (creative) open
                    (screenshots)
  --open-menu <m>   start with a menu open: pause, options, title, create, lan or multiplayer (screenshots)
  --open-block x,y,z  open the furnace, chest, brewing stand, enchanting
                    table, anvil or smithing table once loaded (screenshots)
  --place x,y,z,b   set a block once loaded (repeatable; y may be ~ for the
                    terrain surface, e.g. 0,~,0,water; b may be a raw block id)
  --health <0..20>  starting health in half hearts (0 opens the death screen)
  --air <0..15>     starting air in seconds
  --food <0..20>    starting hunger in half drumsticks (no saturation)
  --xp <level>      starting experience level
  --effect e[,s[,a]]  start with a status effect for s seconds (default
                    30) at amplifier a (default 0; repeatable)
  --orbs <points>   spawn experience orbs worth that many points in front of
                    the player once loaded (repeatable)
  --give item[,n]   add n (default 1) of an item to the inventory at startup
                    (repeatable; e.g. --give iron_pickaxe --give coal,16)
  --drop item[,n]   throw n of an item in front of the player once loaded
                    (repeatable; like --give)
  --wear item       put on a piece of armor at startup (repeatable)
  --enchant e[,l]   enchant the first hotbar stack (or a book there) with
                    level l (default 1) of enchantment e (repeatable)
  --cart x,y,z,kind spawn a cart on rails, or a named boat/raft (repeatable)
  --ride name      ride a scripted boat or saddled/tamed animal (screenshots)
  --spawn kind,x,y,z[,material[,glint]]
                    spawn a mob once loaded (repeatable; any mob name, such as
                    zombie or magma_cube; y may be ~
                    for the terrain surface, e.g. zombie,4,~,10). material
                    (leather, chainmail, iron, gold, diamond, netherite) and
                    glint equip a zombie or skeleton
  --wait <secs>     with --screenshot: keep simulating this long first
  --time <0..1>     starting time of day (0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight)
  --weather <w>     start with clear skies or rain (clear, rain)
  --dimension <d>   start in overworld, nether or end (arriving through a
                    portal unless --pose is given)
  --screenshot <f>  wait for the world to load, save a PNG and exit
  --camera <first|third|front>  starting camera perspective (F5 cycles in game)
  --pose x,y,z,yaw,pitch  start flying at this position (degrees)
  --mute            start with sound muted (M toggles in game)
  --no-hud          start with the HUD and hand hidden (F1 toggles in game)
  --volume <0..1>   master volume (default: 1, or the saved option)
  --export-sounds   write every synthesized sound to target/sounds/*.wav with stats, and exit\n  --export-music    render original seeded music to target/music/*.wav with stats, and exit";

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seed: None,
        host_lan: None,
        join: None,
        profile: None,
        agent_listen: None,
        agent_token: None,
        agent_cheats: false,
        open_console: false,
        split_screen: Vec::new(),
        split_side: false,
        world: None,
        data_dir: None,
        render_distance: None,
        no_vsync: false,
        enhanced_graphics: None,
        new_world: false,
        bench: false,
        screenshot: None,
        camera: Default::default(),
        ride: None,
        bench_render: false,
        debug_overlay: false,
        mode: None,
        open_inventory: false,
        open_trading: false,
        pad_player: None,
        inventory_search: None,
        open_menu: None,
        time: None,
        weather: None,
        dimension: None,
        place: Vec::new(),
        open_block: None,
        carts: Vec::new(),
        spawn: Vec::new(),
        wait: 0.0,
        pose: None,
        health: None,
        air: None,
        food: None,
        xp: None,
        effects: Vec::new(),
        orbs: Vec::new(),
        give: Vec::new(),
        wear: Vec::new(),
        enchants: Vec::new(),
        drop: Vec::new(),
        mute: false,
        no_hud: false,
        volume: None,
        export_sounds: false,
        export_music: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--host-lan" => args.host_lan = Some(value("--host-lan")?.parse().map_err(|_| "bad --host-lan IP:PORT")?),
            "--join" => args.join = Some(value("--join")?),
            "--profile" => {
                let profile = value("--profile")?;
                if !voxelcraft::control::valid_name(&profile) {
                    return Err("invalid --profile (1..24 letters, digits, underscores)".into());
                }
                args.profile = Some(profile);
            }
            "--agent-listen" => {
                args.agent_listen = Some(value("--agent-listen")?.parse().map_err(|_| "bad --agent-listen IP:PORT")?)
            }
            "--agent-token" => args.agent_token = Some(value("--agent-token")?),
            "--agent-cheats" => args.agent_cheats = true,
            "--split-screen" => {
                let v = value("--split-screen")?;
                args.split_screen = v.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
                if args.split_screen.len() > 3 || !args.split_screen.iter().all(|n| voxelcraft::control::valid_name(n))
                {
                    return Err(format!("--split-screen needs up to 3 player names (got {v})"));
                }
            }
            "--split-layout" => {
                args.split_side = match value("--split-layout")?.as_str() {
                    "side" => true,
                    "stacked" => false,
                    _ => return Err("--split-layout needs stacked or side".into()),
                }
            }
            "--open-console" => args.open_console = true,
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|_| "bad seed")?),
            "--world" => args.world = Some(value("--world")?),
            "--data-dir" => args.data_dir = Some(value("--data-dir")?.into()),
            "--rd" => args.render_distance = Some(value("--rd")?.parse::<i32>().map_err(|_| "bad --rd")?.clamp(2, 32)),
            "--no-vsync" => args.no_vsync = true,
            "--graphics" => {
                args.enhanced_graphics = Some(match value("--graphics")?.as_str() {
                    "enhanced" => true,
                    "classic" => false,
                    _ => return Err("--graphics: expected enhanced or classic".into()),
                });
            }
            "--new" => args.new_world = true,
            "--bench" => args.bench = true,
            "--bench-render" => args.bench_render = true,
            "--f3" => args.debug_overlay = true,
            "--inventory-search" => args.inventory_search = Some(value("--inventory-search")?),
            "--open-inventory" => args.open_inventory = true,
            "--open-trading" => args.open_trading = true,
            "--pad-player" => {
                let m = value("--pad-player")?;
                if !matches!(m.as_str(), "play" | "pause" | "inventory" | "crafting" | "palette") {
                    return Err(format!("--pad-player: expected play, pause, inventory, crafting or palette, got {m}"));
                }
                args.pad_player = Some(m);
            }
            "--open-menu" => {
                let m = value("--open-menu")?;
                if !matches!(m.as_str(), "pause" | "options" | "title" | "create" | "lan" | "multiplayer") {
                    return Err(format!(
                        "--open-menu: expected pause, options, title, create, lan or multiplayer, got {m}"
                    ));
                }
                args.open_menu = Some(m);
            }
            "--creative" => args.mode = Some(app::GameMode::Creative),
            "--survival" => args.mode = Some(app::GameMode::Survival),
            "--open-block" => {
                let v = value("--open-block")?;
                let n: Vec<i32> = v
                    .split(',')
                    .map(|s| s.trim().parse().map_err(|_| format!("--open-block needs x,y,z (got {v})")))
                    .collect::<Result<_, _>>()?;
                let &[x, y, z] = &n[..] else { return Err(format!("--open-block needs x,y,z (got {v})")) };
                args.open_block = Some(glam::IVec3::new(x, y, z));
            }
            "--place" => {
                let v = value("--place")?;
                let parts: Vec<&str> = v.split(',').collect();
                let bad = || format!("--place needs x,y,z,block (got {v})");
                if parts.len() != 4 {
                    return Err(bad());
                }
                // `~` for y means "on the terrain surface".
                let n: Vec<i32> = parts[..3]
                    .iter()
                    .map(|s| if s.trim() == "~" { Ok(i32::MIN) } else { s.trim().parse().map_err(|_| bad()) })
                    .collect::<Result<_, _>>()?;
                // A block name, or a raw id for oriented states (stairs facing east...).
                let name = parts[3].trim();
                let block = (name
                    .parse::<u16>()
                    .ok()
                    .filter(|&id| (id as usize) < world::block::STATE_CAPACITY)
                    .map(world::block::Block))
                .filter(|b| *b == world::block::Block::AIR || b.kind() != world::block::RenderKind::Invisible)
                .or_else(|| world::block::Block::from_name(name))
                .ok_or_else(bad)?;
                args.place.push((glam::IVec3::new(n[0], n[1], n[2]), block));
            }
            "--ride" => args.ride = Some(value("--ride")?.replace('_', " ")),
            "--cart" => {
                let v = value("--cart")?;
                let parts: Vec<_> = v.split(',').collect();
                if parts.len() != 4 {
                    return Err("--cart needs x,y,z,rideable|chest|hopper|tnt".into());
                }
                let n: Vec<i32> = parts[..3]
                    .iter()
                    .map(|s| s.parse())
                    .collect::<Result<_, _>>()
                    .map_err(|_| "--cart coordinates must be integers")?;
                let kind = match parts[3] {
                    "rideable" => entity::minecart::CartKind::Rideable,
                    "chest" => entity::minecart::CartKind::Chest,
                    "hopper" => entity::minecart::CartKind::Hopper,
                    "tnt" => entity::minecart::CartKind::Tnt,
                    name => item::Item::from_name(name)
                        .and_then(entity::minecart::CartKind::from_item)
                        .ok_or("unknown vehicle kind")?,
                };
                args.carts.push((kind, glam::IVec3::new(n[0], n[1], n[2])));
            }
            "--spawn" => {
                let v = value("--spawn")?;
                let parts: Vec<&str> = v.split(',').map(str::trim).collect();
                let bad = || format!("--spawn needs kind,x,y,z (got {v})");
                if !(4..=6).contains(&parts.len()) {
                    return Err(bad());
                }
                let kind = entity::MobKind::from_name(parts[0]).ok_or_else(bad)?;
                let n: Vec<i32> = parts[1..4]
                    .iter()
                    .map(|s| if *s == "~" { Ok(i32::MIN) } else { s.parse().map_err(|_| bad()) })
                    .collect::<Result<_, _>>()?;
                let armor = if parts.len() >= 5 {
                    if !(kind.is_zombie() || kind == entity::MobKind::Skeleton) {
                        return Err("--spawn armor is only for zombies and skeletons".into());
                    }
                    let material = entity::armor::ArmorKind::from_name(parts[4])
                        .ok_or_else(|| format!("--spawn: unknown armor {}", parts[4]))?;
                    let glint = if parts.len() == 6 {
                        if parts[5] != "glint" {
                            return Err("--spawn: expected glint after the armor material".into());
                        }
                        true
                    } else {
                        false
                    };
                    Some(entity::armor::Equipped { kind: material, glint })
                } else {
                    None
                };
                args.spawn.push((kind, glam::IVec3::new(n[0], n[1], n[2]), armor));
            }
            "--wait" => args.wait = value("--wait")?.parse().map_err(|_| "bad --wait")?,
            "--time" => args.time = Some(value("--time")?.parse::<f64>().map_err(|_| "bad --time")?.rem_euclid(1.0)),
            "--weather" => {
                args.weather = Some(match value("--weather")?.as_str() {
                    "rain" => true,
                    "clear" => false,
                    _ => return Err("--weather needs clear or rain".into()),
                })
            }
            "--dimension" => {
                let v = value("--dimension")?;
                let dim = world::terrain::Dimension::from_name(&v);
                args.dimension = Some(dim.ok_or(format!("--dimension: expected overworld, nether or end, got {v}"))?);
            }
            "--health" => args.health = Some(value("--health")?.parse().map_err(|_| "bad --health")?),
            "--air" => args.air = Some(value("--air")?.parse().map_err(|_| "bad --air")?),
            "--food" => args.food = Some(value("--food")?.parse().map_err(|_| "bad --food")?),
            "--xp" => args.xp = Some(value("--xp")?.parse().map_err(|_| "bad --xp")?),
            "--effect" => {
                let v = value("--effect")?;
                let mut parts = v.split(',');
                let effect = parts
                    .next()
                    .and_then(voxelcraft::simulation::effects::Effect::from_id)
                    .ok_or("bad --effect name")?;
                let secs = parts.next().map_or(Ok(30), str::parse).map_err(|_| "bad --effect seconds")?;
                let amp = parts.next().map_or(Ok(0), str::parse).map_err(|_| "bad --effect amplifier")?;
                args.effects.push((effect, secs, amp));
            }
            "--orbs" => args.orbs.push(value("--orbs")?.parse().map_err(|_| "bad --orbs")?),
            flag @ ("--give" | "--drop") => {
                let v = value(flag)?;
                let (name, count) = v.split_once(',').unwrap_or((&v, "1"));
                let item = item::Item::from_name(name.trim()).ok_or(format!("{flag}: unknown item {name}"))?;
                let count = count.trim().parse().map_err(|_| format!("{flag}: bad count in {v}"))?;
                if flag == "--give" { &mut args.give } else { &mut args.drop }.push((item, count));
            }
            "--enchant" => {
                let v = value("--enchant")?;
                let (name, level) = v.split_once(',').unwrap_or((&v, "1"));
                let e = enchant::Enchantment::from_name(name.trim()).ok_or(format!("--enchant: unknown {name}"))?;
                args.enchants.push((e, level.trim().parse().map_err(|_| format!("--enchant: bad level in {v}"))?));
            }
            "--wear" => {
                let v = value("--wear")?;
                let item = item::Item::from_name(v.trim()).filter(|i| i.as_armor().is_some());
                args.wear.push(item.ok_or(format!("--wear: not armor: {v}"))?);
            }
            "--camera" => {
                args.camera = match value("--camera")?.as_str() {
                    "first" => voxelcraft::camera::CameraMode::First,
                    "third" => voxelcraft::camera::CameraMode::Third,
                    "front" => voxelcraft::camera::CameraMode::Front,
                    _ => return Err("--camera must be first, third or front".into()),
                }
            }
            "--screenshot" => args.screenshot = Some(value("--screenshot")?),
            "--pose" => {
                let v: Vec<f64> = value("--pose")?.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                args.pose = Some(v.try_into().map_err(|_| "--pose needs x,y,z,yaw,pitch")?);
            }
            "--mute" => args.mute = true,
            "--no-hud" => args.no_hud = true,
            "--volume" => {
                args.volume = Some(value("--volume")?.parse::<f32>().map_err(|_| "bad --volume")?.clamp(0.0, 1.0))
            }
            "--export-music" => args.export_music = true,
            "--export-sounds" => args.export_sounds = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    if let Some(world) = &args.world {
        data::validate_world_name(world)?;
    }
    Ok(args)
}

impl Args {
    /// Forgets the options that set up one world (`--give`, `--pose`, ...)
    /// once it has loaded, so worlds picked later start as saved.
    pub fn clear_one_shot(&mut self) {
        self.new_world = false;
        self.host_lan = None;
        self.join = None;
        self.seed = None;
        self.mode = None;
        self.open_inventory = false;
        self.open_trading = false;
        self.pad_player = None;
        self.inventory_search = None;
        self.open_console = false;
        self.open_menu = None;
        self.time = None;
        self.weather = None;
        self.dimension = None;
        self.place.clear();
        self.carts.clear();
        self.open_block = None;
        self.spawn.clear();
        self.pose = None;
        self.health = None;
        self.air = None;
        self.food = None;
        self.xp = None;
        self.effects.clear();
        self.orbs.clear();
        self.give.clear();
        self.wear.clear();
        self.enchants.clear();
        self.drop.clear();
    }
}

fn main() {
    if std::env::args().len() == 2 {
        match std::env::args().nth(1).as_deref() {
            Some("--version") => {
                println!("VoxelCraft {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return;
            }
            _ => {}
        }
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    if args.export_music {
        if let Err(e) = audio::export_music("target/music", args.seed.unwrap_or(42)) {
            eprintln!("music export failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.export_sounds {
        if let Err(e) = audio::export_sounds("target/sounds") {
            eprintln!("export failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.bench {
        env_logger::init();
        bench::run(args.seed.unwrap_or(12345), args.render_distance.unwrap_or(8));
        return;
    }
    let data_dir = data::prepare(args.data_dir.as_deref()).unwrap_or_else(|e| {
        eprintln!("Could not prepare VoxelCraft's data folder: {e}");
        std::process::exit(1);
    });
    data::init_logging(&data_dir);
    log::info!("VoxelCraft {} ({}/{})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    log::info!("data folder: {}", data_dir.display());
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app::App::new(args, data_dir.join("saves"))).expect("event loop");
}
