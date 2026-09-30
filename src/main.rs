mod app;
mod inventory;
mod bench;
mod entity;
mod mesh;
mod physics;
mod player;
mod render;
mod workers;
mod world;

use winit::event_loop::{ControlFlow, EventLoop};

pub struct Args {
    pub seed: Option<u64>,
    pub world: String,
    pub render_distance: i32,
    pub no_vsync: bool,
    pub new_world: bool,
    pub bench: bool,
    pub screenshot: Option<String>,
    pub bench_render: bool,
    pub debug_overlay: bool,
    pub mode: Option<app::GameMode>,
    pub open_inventory: bool,
    /// Starting time of day, 0..1 (0 sunrise, 0.25 noon, 0.75 midnight).
    pub time: Option<f64>,
    /// Blocks to set once the world has loaded (debugging/screenshots).
    pub place: Vec<(glam::IVec3, world::block::Block)>,
    /// Mobs to spawn once the world has loaded (y = i32::MIN: surface).
    pub spawn: Vec<(entity::MobKind, glam::IVec3)>,
    /// Seconds to keep running after loading before `--screenshot`.
    pub wait: f64,
    /// x,y,z,yaw_deg,pitch_deg
    pub pose: Option<[f64; 5]>,
}

const USAGE: &str = "\
voxelcraft [options]
  --seed <n>        world seed (new worlds only)
  --world <name>    save name under ./saves (default: world)
  --rd <chunks>     render distance in 32-block chunks (default: 8)
  --new             ignore any existing save and start a fresh world
  --no-vsync        uncapped frame rate
  --bench           headless terrain generation + meshing benchmark
  --bench-render    load the world, render a 360° sweep offscreen, report frame times
  --creative, --survival  game mode (default: survival, or the saved mode)
  --f3              start with the debug overlay open
  --open-inventory  start with the inventory screen open (screenshots)
  --place x,y,z,b   set a block once loaded (repeatable; y may be ~ for the
                    terrain surface, e.g. 0,~,0,water)
  --spawn kind,x,y,z  spawn a mob once loaded (repeatable; pig or zombie, y may
                    be ~ for the terrain surface, e.g. zombie,4,~,10)
  --wait <secs>     with --screenshot: keep simulating this long first
  --time <0..1>     starting time of day (0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight)
  --screenshot <f>  wait for the world to load, save a PNG and exit
  --pose x,y,z,yaw,pitch  start flying at this position (degrees)";

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seed: None,
        world: "world".into(),
        render_distance: 8,
        no_vsync: false,
        new_world: false,
        bench: false,
        screenshot: None,
        bench_render: false,
        debug_overlay: false,
        mode: None,
        open_inventory: false,
        time: None,
        place: Vec::new(),
        spawn: Vec::new(),
        wait: 0.0,
        pose: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|_| "bad seed")?),
            "--world" => args.world = value("--world")?,
            "--rd" => args.render_distance = value("--rd")?.parse().map_err(|_| "bad --rd")?,
            "--no-vsync" => args.no_vsync = true,
            "--new" => args.new_world = true,
            "--bench" => args.bench = true,
            "--bench-render" => args.bench_render = true,
            "--f3" => args.debug_overlay = true,
            "--open-inventory" => args.open_inventory = true,
            "--creative" => args.mode = Some(app::GameMode::Creative),
            "--survival" => args.mode = Some(app::GameMode::Survival),
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
                let block = world::block::Block::from_name(parts[3].trim()).ok_or_else(bad)?;
                args.place.push((glam::IVec3::new(n[0], n[1], n[2]), block));
            }
            "--spawn" => {
                let v = value("--spawn")?;
                let parts: Vec<&str> = v.split(',').map(str::trim).collect();
                let bad = || format!("--spawn needs kind,x,y,z (got {v})");
                if parts.len() != 4 {
                    return Err(bad());
                }
                let kind = entity::MobKind::from_name(parts[0]).ok_or_else(bad)?;
                let n: Vec<i32> = parts[1..]
                    .iter()
                    .map(|s| if *s == "~" { Ok(i32::MIN) } else { s.parse().map_err(|_| bad()) })
                    .collect::<Result<_, _>>()?;
                args.spawn.push((kind, glam::IVec3::new(n[0], n[1], n[2])));
            }
            "--wait" => args.wait = value("--wait")?.parse().map_err(|_| "bad --wait")?,
            "--time" => args.time = Some(value("--time")?.parse::<f64>().map_err(|_| "bad --time")?.rem_euclid(1.0)),
            "--screenshot" => args.screenshot = Some(value("--screenshot")?),
            "--pose" => {
                let v: Vec<f64> = value("--pose")?.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                args.pose = Some(v.try_into().map_err(|_| "--pose needs x,y,z,yaw,pitch")?);
            }
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    args.render_distance = args.render_distance.clamp(2, 32);
    Ok(args)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,voxelcraft=info")).init();
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    if args.bench {
        bench::run(args.seed.unwrap_or(12345), args.render_distance);
        return;
    }
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app::App::new(args)).expect("event loop");
}
