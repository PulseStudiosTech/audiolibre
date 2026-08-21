//! AudioLibre daemon — starts the audio engine and IPC server.

use audiolibre_engine::{DeviceManager, Mixer, MixerChannel, Router};
use audiolibre_engine::ipc::IpcServer;
use std::sync::Arc;

#[cfg(unix)]
const SOCKET_PATH: &str = "/tmp/audiolibre.sock";

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("AudioLibre daemon starting");

    let devices = Arc::new(DeviceManager::new());
    let mixer = Arc::new(Mixer::new());
    let router = Arc::new(Router::new());

    // Print available devices on startup
    match devices.list_inputs() {
        Ok(inputs) => {
            log::info!("Input devices ({}):", inputs.len());
            for d in &inputs {
                log::info!("  [{}] {} @ {}Hz {}ch{}",
                    if d.is_default { "default" } else { "       " },
                    d.name, d.sample_rate, d.channels,
                    "");
            }
        }
        Err(e) => log::warn!("Could not enumerate inputs: {}", e),
    }

    match devices.list_outputs() {
        Ok(outputs) => {
            log::info!("Output devices ({}):", outputs.len());
            for d in &outputs {
                log::info!("  [{}] {} @ {}Hz {}ch",
                    if d.is_default { "default" } else { "       " },
                    d.name, d.sample_rate, d.channels);
            }
        }
        Err(e) => log::warn!("Could not enumerate outputs: {}", e),
    }

    // Create default mixer channels
    let mic_ch = MixerChannel::new("Microphone", "main");
    let desktop_ch = MixerChannel::new("Desktop Audio", "main");
    let music_ch = MixerChannel::new("Music", "stream");
    let game_ch = MixerChannel::new("Game", "stream");

    let mic_id = mixer.add_channel(mic_ch);
    let _desktop_id = mixer.add_channel(desktop_ch);
    let _music_id = mixer.add_channel(music_ch);
    let _game_id = mixer.add_channel(game_ch);

    log::info!("Default channels created (mic={}, desktop, music, game)", mic_id);
    log::info!("Buses: {:?}", mixer.get_buses());

    // Start IPC server
    let server = IpcServer::new(SOCKET_PATH, mixer.clone(), router.clone(), devices.clone());
    log::info!("Starting IPC server at {}", SOCKET_PATH);

    if let Err(e) = server.run().await {
        log::error!("IPC server error: {}", e);
        std::process::exit(1);
    }
}
