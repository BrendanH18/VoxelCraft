//! Agent client: one command or a persistent JSON-lines conversation on stdin/stdout.
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;
use voxelcraft::control::{VERSION, read_response, valid_name};

const HELP: &str = "voxelcraft-agent [--connect IP:PORT] [--player NAME] [--token TOKEN] [command ...]\n\
Default: 127.0.0.1:4242, player agent. With no command, read commands from stdin.\n\
Each command produces one JSON response on stdout. Use 'help' for gameplay commands.\n\
Start a host with: voxelcraft --world world --agent-listen 127.0.0.1:4242";

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = "127.0.0.1:4242".to_string();
    let mut player = "agent".to_string();
    let mut token = None;
    let mut args = std::env::args().skip(1);
    let mut command = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--connect" => address = args.next().ok_or("--connect needs IP:PORT")?,
            "--player" => player = args.next().ok_or("--player needs NAME")?,
            "--token" => token = Some(args.next().ok_or("--token needs TOKEN")?),
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            _ => {
                command.push(arg);
                command.extend(args);
                break;
            }
        }
    }
    if !valid_name(&player) {
        return Err("player name must be 1..24 letters, digits or underscores".into());
    }
    let send = |command: &str| -> Result<bool, Box<dyn std::error::Error>> {
        if command.len() > 1024 {
            return Err("command exceeds 1024 bytes".into());
        }
        let mut stream = TcpStream::connect(&address)?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let request = json!({"version":VERSION,"player":player,"token":token,"command":command});
        writeln!(stream, "{request}")?;
        let line = read_response(&mut BufReader::new(stream))?.ok_or("host disconnected")?;
        let value: Value = serde_json::from_str(&line)?;
        println!("{value}");
        io::stdout().flush()?;
        Ok(value["ok"] == true)
    };
    if command.is_empty() {
        for line in io::stdin().lock().lines() {
            let line = line?;
            if !line.trim().is_empty() {
                send(&line)?;
            }
        }
    } else if !send(&command.join(" "))? {
        std::process::exit(1);
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("voxelcraft-agent: {e}");
        std::process::exit(1);
    }
}
