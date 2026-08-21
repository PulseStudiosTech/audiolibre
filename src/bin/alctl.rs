//! AudioLibre CLI control tool — sends commands to the daemon over IPC.
//! Usage: alctl <command> [args...]
//! Examples:
//!   alctl list-inputs
//!   alctl list-channels
//!   alctl set-volume <channel-id> <0.0-1.0>
//!   alctl set-mute <channel-id> <true|false>

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use serde_json::Value;

const SOCKET_PATH: &str = "/tmp/audiolibre.sock";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: alctl <command> [args...]");
        eprintln!("Commands: list-inputs, list-outputs, list-channels, list-buses, list-routes");
        eprintln!("          set-volume <id> <vol>, set-mute <id> <bool>, set-bus <id> <bus>");
        eprintln!("          add-channel <name> <bus>, remove-channel <id>");
        eprintln!("          add-route <app> <channel-id>, remove-route <app>");
        std::process::exit(1);
    }

    let cmd_json = match build_command(&args) {
        Some(j) => j,
        None => {
            eprintln!("Unknown command or missing args: {:?}", args);
            std::process::exit(1);
        }
    };

    let mut stream = match UnixStream::connect(SOCKET_PATH) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not connect to daemon at {}: {}", SOCKET_PATH, e);
            eprintln!("Is audiod running?");
            std::process::exit(1);
        }
    };

    let mut line = cmd_json.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();

    let v: Value = serde_json::from_str(&response).unwrap_or(Value::Null);
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or(response));

    if v.get("ok").and_then(|b| b.as_bool()) == Some(false) {
        std::process::exit(1);
    }
}

fn build_command(args: &[String]) -> Option<Value> {
    Some(match args[0].as_str() {
        "list-inputs"   => serde_json::json!({"cmd": "list_inputs"}),
        "list-outputs"  => serde_json::json!({"cmd": "list_outputs"}),
        "list-channels" => serde_json::json!({"cmd": "list_channels"}),
        "list-buses"    => serde_json::json!({"cmd": "list_buses"}),
        "list-routes"   => serde_json::json!({"cmd": "list_routes"}),
        "list-mics"     => serde_json::json!({"cmd": "list_mics"}),
        "get-output"    => serde_json::json!({"cmd": "get_active_output"}),
        "add-channel" if args.len() >= 3 =>
            serde_json::json!({"cmd": "add_channel", "name": args[1], "output_bus": args[2]}),
        "remove-channel" if args.len() >= 2 =>
            serde_json::json!({"cmd": "remove_channel", "id": args[1]}),
        "set-volume" if args.len() >= 3 =>
            serde_json::json!({"cmd": "set_volume", "id": args[1], "volume": args[2].parse::<f32>().unwrap_or(1.0)}),
        "set-mute" if args.len() >= 3 =>
            serde_json::json!({"cmd": "set_mute", "id": args[1], "muted": args[2] == "true"}),
        "set-bus" if args.len() >= 3 =>
            serde_json::json!({"cmd": "set_output_bus", "id": args[1], "bus": args[2]}),
        "add-route" if args.len() >= 3 =>
            serde_json::json!({"cmd": "add_route", "app_name": args[1], "channel_id": args[2]}),
        "remove-route" if args.len() >= 2 =>
            serde_json::json!({"cmd": "remove_route", "app_name": args[1]}),
        _ => return None,
    })
}
