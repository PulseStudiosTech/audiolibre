//! AudioLibre daemon — starts the audio engine and IPC server.
use audiolibre_engine::{AudioEngine, CaptureSource, DeviceManager, Mixer, MixerChannel, OutputDest, Router};
use audiolibre_engine::ipc::IpcServer;
use audiolibre_engine::detect::{detect_active_output, detect_microphones};
use std::sync::Arc;

#[cfg(unix)]
const SOCKET_PATH: &str = audiolibre_engine::ipc::SOCKET_PATH;

#[tokio::main]
async fn main() {
    env_logger::init();
    log::info!("AudioLibre daemon starting");

    let devices = Arc::new(DeviceManager::new());
    let mixer = Arc::new(Mixer::new());
    let router = Arc::new(Router::new());

    // ── Detect mic and active output ─────────────────────────────────────────
    let mic_name = detect_microphones(&devices)
        .ok()
        .and_then(|v| v.into_iter().next().map(|d| d.device.name));
    let output_name = detect_active_output(&devices)
        .ok()
        .map(|d| d.name);

    if let Some(ref n) = mic_name {
        log::info!("Detected microphone: {}", n);
    } else {
        log::warn!("No standalone microphone detected");
    }
    if let Some(ref n) = output_name {
        log::info!("Active output: {}", n);
    } else {
        log::warn!("No default output device");
    }

    // ── Create mixer channels ─────────────────────────────────────────────────
    let mut mic_ch = MixerChannel::new("Microphone", "main");
    if let Some(ref n) = mic_name {
        mic_ch.input_device = Some(n.clone());
    }
    let mic_id = mixer.add_channel(mic_ch);

    let desktop_id = mixer.add_channel(MixerChannel::new("Desktop Audio", "main"));
    let _music_id  = mixer.add_channel(MixerChannel::new("Music",         "stream"));
    let _game_id   = mixer.add_channel(MixerChannel::new("Game",          "stream"));

    log::info!("Channels: mic={}, desktop={}", mic_id, desktop_id);

    // ── Start audio engine ────────────────────────────────────────────────────
    let engine = AudioEngine::new(mixer.clone());
    let mut sources = Vec::new();
    let mut outputs = Vec::new();

    if let Some(name) = mic_name {
        sources.push(CaptureSource { device_name: name, channel_id: mic_id.clone() });
    }
    if let Some(name) = output_name {
        outputs.push(OutputDest { device_name: name.clone(), bus: "main".to_string() });
        outputs.push(OutputDest { device_name: name, bus: "headphones".to_string() });
    }

    let _handle = match engine.start(sources, outputs) {
        Ok(h) => {
            log::info!("Audio engine running");
            Some(h)
        }
        Err(e) => {
            log::warn!("Audio engine could not start (no hardware?): {}", e);
            None
        }
    };

    // ── IPC server ────────────────────────────────────────────────────────────
    let server = IpcServer::new(
        audiolibre_engine::ipc::SOCKET_PATH,
        mixer.clone(),
        router.clone(),
        devices.clone(),
    );
    log::info!("IPC server starting at {}", audiolibre_engine::ipc::SOCKET_PATH);

    if let Err(e) = server.run().await {
        log::error!("IPC server error: {}", e);
        std::process::exit(1);
    }
}
