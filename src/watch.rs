//! DeviceWatcher — polls for audio device changes every 2 seconds.
//!
//! Emits DeviceEvent::Added/Removed events so the engine can hot-swap streams.
//! On Windows, a proper IMMNotificationClient would be more efficient, but
//! polling is cross-platform and good enough for ~2s response time.

use std::collections::HashSet;
use std::thread;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait};
use crossbeam_channel::{unbounded, Receiver, Sender};

#[derive(Debug, Clone)]
pub enum DeviceEvent {
    Added   { name: String, is_input: bool },
    Removed { name: String, is_input: bool },
}

pub struct DeviceWatcher {
    rx: Receiver<DeviceEvent>,
    // Thread is kept alive implicitly — it loops forever until process exits.
    // We don't join it; device watching is a background daemon.
}

impl DeviceWatcher {
    /// Start the device watcher background thread.
    pub fn start() -> Self {
        let (tx, rx) = unbounded::<DeviceEvent>();
        thread::Builder::new()
            .name("al-device-watch".into())
            .spawn(move || run_watcher(tx))
            .expect("failed to start device watcher");
        DeviceWatcher { rx }
    }

    /// Non-blocking: returns the next pending event, or None.
    pub fn try_recv(&self) -> Option<DeviceEvent> {
        self.rx.try_recv().ok()
    }

    /// Returns all pending events since last call.
    pub fn drain(&self) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        while let Some(e) = self.try_recv() {
            events.push(e);
        }
        events
    }
}

fn run_watcher(tx: Sender<DeviceEvent>) {
    let host = cpal::default_host();

    let mut known_inputs: HashSet<String> = snapshot_inputs(&host);
    let mut known_outputs: HashSet<String> = snapshot_outputs(&host);

    loop {
        thread::sleep(Duration::from_secs(2));

        let current_inputs = snapshot_inputs(&host);
        emit_diff(&tx, &known_inputs, &current_inputs, true);
        known_inputs = current_inputs;

        let current_outputs = snapshot_outputs(&host);
        emit_diff(&tx, &known_outputs, &current_outputs, false);
        known_outputs = current_outputs;
    }
}

fn snapshot_inputs(host: &cpal::Host) -> HashSet<String> {
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn snapshot_outputs(host: &cpal::Host) -> HashSet<String> {
    host.output_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn emit_diff(
    tx: &Sender<DeviceEvent>,
    before: &HashSet<String>,
    after: &HashSet<String>,
    is_input: bool,
) {
    for added in after.difference(before) {
        log::info!("Device {}: {} ({})", if is_input { "input" } else { "output" }, added,
            if is_input { "added" } else { "added" });
        let _ = tx.send(DeviceEvent::Added { name: added.clone(), is_input });
    }
    for removed in before.difference(after) {
        log::warn!("Device {}: {} (removed)", if is_input { "input" } else { "output" }, removed);
        let _ = tx.send(DeviceEvent::Removed { name: removed.clone(), is_input });
    }
}
