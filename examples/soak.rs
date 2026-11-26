//! v1.0 Track 3: the headless long-session soak (`voxelcraft::soak`).
//!
//! cargo run --profile ci --locked --no-default-features --example soak -- \
//!     --hours 3 --csv target/soak/soak.csv
//!
//! Prints a line per sample and the run's summary; the CSV has every value.
use std::path::PathBuf;

use voxelcraft::soak::{self, Options};

fn main() {
    let mut o = Options { csv: Some(PathBuf::from("target/soak/soak.csv")), ..Options::default() };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--hours" => o.hours = value().parse().expect("--hours"),
            "--leg-minutes" => o.leg_minutes = value().parse().expect("--leg-minutes"),
            "--sample-seconds" => o.sample_seconds = value().parse().expect("--sample-seconds"),
            "--autosave-seconds" => o.autosave_seconds = value().parse().expect("--autosave-seconds"),
            "--rd" => o.render_distance = value().parse().expect("--rd"),
            "--seed" => o.seed = value().parse().expect("--seed"),
            "--data-dir" => o.data_dir = PathBuf::from(value()),
            "--csv" => o.csv = Some(PathBuf::from(value())),
            "--chunks-file" => o.regions = false,
            _ => panic!(
                "usage: soak [--hours H] [--leg-minutes M] [--sample-seconds S] [--autosave-seconds S] \
                 [--rd N] [--seed N] [--data-dir DIR] [--csv FILE] [--chunks-file]"
            ),
        }
    }
    let watch = [
        "sim_minutes",
        "wall_seconds",
        "rss_kb",
        "threads",
        "tick_ms_mean",
        "tick_ms_max",
        "save_ms",
        "save_bytes",
        "w.chunks.loaded",
        "w.chunks.saved_unloaded",
        "e.entities.mobs",
        "e.stored.entities",
    ];
    let summary = soak::run(&o, |s| {
        let values: Vec<String> =
            watch.iter().map(|k| format!("{}={}", k, s.get(k).map_or(-1.0, |v| (v * 10.0).round() / 10.0))).collect();
        println!("{} {} {}", s.leg, s.dimension, values.join(" "));
    });
    match summary {
        Ok(s) => println!("done: {s:?}"),
        Err(e) => {
            eprintln!("soak: {e}");
            std::process::exit(1);
        }
    }
}
