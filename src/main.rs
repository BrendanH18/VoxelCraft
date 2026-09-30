mod app;
mod bench;
mod mesh;
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
  --f3              start with the debug overlay open
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
