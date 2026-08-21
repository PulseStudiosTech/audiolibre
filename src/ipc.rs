//! Unix socket + JSON IPC server for GUI ↔ engine communication.
//! The Flutter UI sends JSON commands; the engine replies with JSON.

use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use crate::{Mixer, Router, DeviceManager};
use crate::error::{AudioLibreError, Result};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    ListInputs,
    ListOutputs,
    ListChannels,
    ListRoutes,
    ListMics,
    GetActiveOutput,
    ListBuses,
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
    fn ok(data: impl Serialize) -> Self {
        Self {
            ok: true,
            data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

pub struct IpcServer {
    socket_path: String,
    mixer: Arc<Mixer>,
    router: Arc<Router>,
    devices: Arc<DeviceManager>,
}

impl IpcServer {
    pub fn new(
        socket_path: impl Into<String>,
        mixer: Arc<Mixer>,
        router: Arc<Router>,
        devices: Arc<DeviceManager>,
    ) -> Self {
        Self {
            socket_path: socket_path.into(),
            mixer,
            router,
            devices,
        }
    }

    pub async fn run(self) -> Result<()> {
        let path = &self.socket_path;
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)
            .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;

        log::info!("IPC server listening on {}", path);

        let mixer = self.mixer.clone();
        let router = self.router.clone();
        let devices = self.devices.clone();

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| AudioLibreError::IpcError(e.to_string()))?;

            let m = mixer.clone();
            let r = router.clone();
            let d = devices.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, m, r, d).await {
                    log::error!("IPC connection error: {}", e);
                }
            });
        }
    }
}

async fn handle_connection(
    stream: UnixStream,
    mixer: Arc<Mixer>,
    router: Arc<Router>,
    devices: Arc<DeviceManager>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let response = match serde_json::from_str::<Command>(&line) {
            Err(e) => Response::err(format!("parse error: {}", e)),
            Ok(cmd) => dispatch(cmd, &mixer, &router, &devices),
        };
        let mut json = serde_json::to_string(&response)?;
        json.push('\n');
        writer.write_all(json.as_bytes()).await?;
    }
    Ok(())
}

fn dispatch(
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
        Command::ListMics => match crate::detect::detect_microphones(&devices) {
            Ok(mics) => Response::ok(mics.iter().map(|m| serde_json::json!({"name": m.device.name, "score": m.score, "reason": m.reason})).collect::<Vec<_>>()),
            Err(e) => Response::err(e.to_string()),
        },
        Command::GetActiveOutput => match crate::detect::detect_active_output(&devices) {
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
