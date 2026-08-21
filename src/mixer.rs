use std::collections::HashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{AudioLibreError, Result};

pub type ChannelId = String;
pub type Sample = f32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerChannel {
    pub id: ChannelId,
    pub name: String,
    pub volume: f32,   // 0.0 – 1.0
    pub muted: bool,
    pub input_device: Option<String>,
    pub output_bus: String, // "main", "stream", "headphones", etc.
}

impl MixerChannel {
    pub fn new(name: impl Into<String>, output_bus: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            volume: 1.0,
            muted: false,
            input_device: None,
            output_bus: output_bus.into(),
        }
    }

    pub fn effective_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume.clamp(0.0, 1.0) }
    }
}

/// Thread-safe audio mixer that holds N channels, each with its own volume/mute state.
/// Audio frames are pulled from each channel's capture thread via crossbeam channels
/// and mixed into output buses.
pub struct Mixer {
    channels: RwLock<HashMap<ChannelId, MixerChannel>>,
    buses: RwLock<Vec<String>>,
}

impl Mixer {
    pub fn new() -> Self {
        let mut buses = Vec::new();
        buses.push("main".to_string());
        buses.push("stream".to_string());
        buses.push("headphones".to_string());
        Self {
            channels: RwLock::new(HashMap::new()),
            buses: RwLock::new(buses),
        }
    }

    pub fn add_channel(&self, channel: MixerChannel) -> ChannelId {
        let id = channel.id.clone();
        self.channels.write().insert(id.clone(), channel);
        id
    }

    pub fn remove_channel(&self, id: &str) -> Result<()> {
        self.channels
            .write()
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| AudioLibreError::MixerError(format!("Channel '{}' not found", id)))
    }

    pub fn set_volume(&self, id: &str, volume: f32) -> Result<()> {
        let mut channels = self.channels.write();
        let ch = channels
            .get_mut(id)
            .ok_or_else(|| AudioLibreError::MixerError(format!("Channel '{}' not found", id)))?;
        ch.volume = volume.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn set_mute(&self, id: &str, muted: bool) -> Result<()> {
        let mut channels = self.channels.write();
        let ch = channels
            .get_mut(id)
            .ok_or_else(|| AudioLibreError::MixerError(format!("Channel '{}' not found", id)))?;
        ch.muted = muted;
        Ok(())
    }

    pub fn set_output_bus(&self, id: &str, bus: impl Into<String>) -> Result<()> {
        let bus = bus.into();
        {
            let buses = self.buses.read();
            if !buses.contains(&bus) {
                return Err(AudioLibreError::MixerError(format!("Bus '{}' not found", bus)));
            }
        }
        let mut channels = self.channels.write();
        let ch = channels
            .get_mut(id)
            .ok_or_else(|| AudioLibreError::MixerError(format!("Channel '{}' not found", id)))?;
        ch.output_bus = bus;
        Ok(())
    }

    pub fn get_channels(&self) -> Vec<MixerChannel> {
        self.channels.read().values().cloned().collect()
    }

    pub fn get_buses(&self) -> Vec<String> {
        self.buses.read().clone()
    }

    /// Mix a batch of per-channel audio frames into bus buffers.
    /// `frames`: map of channel_id → interleaved f32 samples for this chunk
    /// Returns: map of bus_name → mixed interleaved f32 samples
    pub fn mix(
        &self,
        frames: &HashMap<ChannelId, Vec<Sample>>,
        chunk_size: usize,
    ) -> HashMap<String, Vec<Sample>> {
        let channels = self.channels.read();
        let buses = self.buses.read();

        let mut output: HashMap<String, Vec<Sample>> =
            buses.iter().map(|b| (b.clone(), vec![0.0f32; chunk_size])).collect();

        for (ch_id, samples) in frames {
            let channel = match channels.get(ch_id) {
                Some(c) => c,
                None => continue,
            };
            let vol = channel.effective_volume();
            if vol == 0.0 { continue; }

            let bus = output.get_mut(&channel.output_bus);
            if let Some(bus_buf) = bus {
                let len = bus_buf.len().min(samples.len());
                for i in 0..len {
                    bus_buf[i] += samples[i] * vol;
                }
            }
        }

        // Soft clamp to [-1.0, 1.0]
        for buf in output.values_mut() {
            for s in buf.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }
        }

        output
    }
}
