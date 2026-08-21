//! IPC server — Unix socket (Linux/macOS) or named pipe (Windows).
//! The Flutter UI sends newline-delimited JSON commands; replies are newline-delimited JSON.

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::{Mixer, Router, DeviceManager};
use crate::error::{AudioLibreError, Result};

// Platform socket path / pipe name
#[cfg(unix)]
pub const SOCKET_PATH: &str = "/tmp/audiolibre.sock";
#[cfg(windows)]
pub const PIPE_NAME: &str = r"\\.\pipe\audiolibre";

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    ListInputs,
    ListOutputs,
    ListChannels,
    ListRoutes,
    ListBuses,
    ListMics,
    GetActiveOutput,
    AddChannel { name: String, output_bus: String },
    RemoveChannel { id: String },
    SetVolume { id: String, volume: f32 },
    SetMute { id: String, muted: bool },
    SetOutputBus { id: String, bus: String },
    AddRoute { app_name: String, channel_id: String },
    RemoveRoute { app_name: String },
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: impl Serialize) -> Self {
        Self {
            ok: true,
            data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

pub fn dispatch(
    cmd: Command,
    mixer: &Mixer,
    router: &Router,
    devices: &DeviceManager,
) -> Response {
    match cmd {
        Command::ListInputs => match devices.list_inputs() {
            Ok(d) => Response::ok(d),
            Err(e) => Response::err(e.to_string()),
        },
        Command::ListOutputs => match devices.list_outputs() {
            Ok(d) => Response::ok(d),
            Err(e) => Response::err(e.to_string()),
        },
        Command::ListChannels => Response::ok(mixer.get_channels()),
        Command::ListBuses => Response::ok(mixer.get_buses()),
        Command::ListRoutes => Response::ok(router.all_routes()),
        Command::ListMics => match crate::detect::detect_microphones(devices) {
            Ok(mics) => Response::ok(mics.iter().map(|m| serde_json::json!({
                "name": m.device.name, "score": m.score, "reason": m.reason
            })).collect::<Vec<_>>()),
            Err(e) => Response::err(e.to_string()),
        },
        Command::GetActiveOutput => match crate::detect::detect_active_output(devices) {
            Ok(d) => Response::ok(d),
            Err(e) => Response::err(e.to_string()),
        },
        Command::AddChannel { name, output_bus } => {
            let ch = crate::MixerChannel::new(name, output_bus);
            let id = mixer.add_channel(ch);
            Response::ok(serde_json::json!({ "id": id }))
        }
        Command::RemoveChannel { id } => match mixer.remove_channel(&id) {
            Ok(()) => Response::ok(serde_json::json!({})),
            Err(e) => Response::err(e.to_string()),
        },
        Command::SetVolume { id, volume } => match mixer.set_volume(&id, volume) {
            Ok(()) => Response::ok(serde_json::json!({})),
            Err(e) => Response::err(e.to_string()),
        },
        Command::SetMute { id, muted } => match mixer.set_mute(&id, muted) {
            Ok(()) => Response::ok(serde_json::json!({})),
            Err(e) => Response::err(e.to_string()),
        },
        Command::SetOutputBus { id, bus } => match mixer.set_output_bus(&id, bus) {
            Ok(()) => Response::ok(serde_json::json!({})),
            Err(e) => Response::err(e.to_string()),
        },
        Command::AddRoute { app_name, channel_id } => {
            router.add_route(app_name, channel_id);
            Response::ok(serde_json::json!({}))
        }
        Command::RemoveRoute { app_name } => match router.remove_route(&app_name) {
            Ok(()) => Response::ok(serde_json::json!({})),
            Err(e) => Response::err(e.to_string()),
        },
    }
}

// ─── Unix IPC server ──────────────────────────────────────────────────────────
#[cfg(unix)]
pub struct IpcServer {
    socket_path: String,
    mixer: Arc<Mixer>,
    router: Arc<Router>,
    devices: Arc<DeviceManager>,
}

#[cfg(unix)]
impl IpcServer {
    pub fn new(
        socket_path: impl Into<String>,
        mixer: Arc<Mixer>,
        router: Arc<Router>,
        devices: Arc<DeviceManager>,
    ) -> Self {
        Self { socket_path: socket_path.into(), mixer, router, devices }
    }

    pub async fn run(self) -> Result<()> {
        use tokio::net::UnixListener;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let path = &self.socket_path;
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)
            .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;

        log::info!("IPC server listening on {}", path);

        loop {
            let (stream, _) = listener.accept().await
                .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;
            let (m, r, d) = (self.mixer.clone(), self.router.clone(), self.devices.clone());
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let response = match serde_json::from_str::<Command>(&line) {
                        Err(e) => Response::err(format!("parse error: {}", e)),
                        Ok(cmd) => dispatch(cmd, &m, &r, &d),
                    };
                    let mut json = serde_json::to_string(&response).unwrap_or_default();
                    json.push('\n');
                    let _ = writer.write_all(json.as_bytes()).await;
                }
            });
        }
    }
}

// ─── Windows Named Pipe IPC server ───────────────────────────────────────────
#[cfg(windows)]
pub struct IpcServer {
    pipe_name: String,
    mixer: Arc<Mixer>,
    router: Arc<Router>,
    devices: Arc<DeviceManager>,
}

#[cfg(windows)]
impl IpcServer {
    pub fn new(
        pipe_name: impl Into<String>,
        mixer: Arc<Mixer>,
        router: Arc<Router>,
        devices: Arc<DeviceManager>,
    ) -> Self {
        Self { pipe_name: pipe_name.into(), mixer, router, devices }
    }

    pub async fn run(self) -> Result<()> {
        use tokio::net::windows::named_pipe::ServerOptions;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        log::info!("IPC server listening on {}", self.pipe_name);

        loop {
            let server = ServerOptions::new()
                .first_pipe_instance(false)
                .create(&self.pipe_name)
                .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;

            server.connect().await
                .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;

            let (m, r, d) = (self.mixer.clone(), self.router.clone(), self.devices.clone());
            tokio::spawn(async move {
                let (reader, mut writer) = tokio::io::split(server);
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let response = match serde_json::from_str::<Command>(&line) {
                        Err(e) => Response::err(format!("parse error: {}", e)),
                        Ok(cmd) => dispatch(cmd, &m, &r, &d),
                    };
                    let mut json = serde_json::to_string(&response).unwrap_or_default();
                    json.push('\n');
                    let _ = writer.write_all(json.as_bytes()).await;
                }
            });
        }
    }
}
