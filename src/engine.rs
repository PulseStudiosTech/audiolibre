//! AudioEngine — manages the full capture→mix→output audio processing loop.
//!
//! Owns all capture streams and output players, feeds frames into the Mixer,
//! and writes mixed bus outputs to the appropriate output devices.

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use crate::{Mixer, MixerChannel, DeviceManager};
use crate::mixer::ChannelId;
use crate::error::{AudioLibreError, Result};

pub type AudioFrame = Vec<f32>;

/// Configuration for one capture source
#[derive(Debug, Clone)]
pub struct CaptureSource {
    pub device_name: String,
    pub channel_id: ChannelId,
}

/// Configuration for one output destination
#[derive(Debug, Clone)]
pub struct OutputDest {
    pub device_name: String,
    pub bus: String, // "main", "stream", "headphones"
}

pub struct AudioEngine {
    mixer: Arc<Mixer>,
    host: cpal::Host,
    running: Arc<parking_lot::RwLock<bool>>,
}

impl AudioEngine {
    pub fn new(mixer: Arc<Mixer>) -> Self {
        Self {
            mixer,
            host: cpal::default_host(),
            running: Arc::new(parking_lot::RwLock::new(false)),
        }
    }

    /// Start the audio processing loop.
    ///
    /// `sources` — devices to capture from, each mapped to a mixer channel
    /// `outputs` — devices to play mixed buses to
    pub fn start(
        &self,
        sources: Vec<CaptureSource>,
        outputs: Vec<OutputDest>,
    ) -> Result<AudioEngineHandle> {
        if *self.running.read() {
            return Err(AudioLibreError::StreamError("Engine already running".into()));
        }
        *self.running.write() = true;

        // For each capture source, start a capture stream that sends frames
        let mut capture_senders: HashMap<ChannelId, Sender<AudioFrame>> = HashMap::new();
        let mut capture_receivers: HashMap<ChannelId, Receiver<AudioFrame>> = HashMap::new();

        let mut streams: Vec<cpal::Stream> = Vec::new();

        for source in &sources {
            let device = match self.host.input_devices()
                .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
                .find(|d| d.name().map(|n| n == source.device_name).unwrap_or(false))
            {
                Some(d) => d,
                None => {
                    log::warn!("Capture device '{}' not found, skipping", source.device_name);
                    continue;
                }
            };

            let config = device.default_input_config()
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

            let (tx, rx) = unbounded::<AudioFrame>();
            let channel_id = source.channel_id.clone();
            capture_senders.insert(channel_id.clone(), tx.clone());
            capture_receivers.insert(channel_id.clone(), rx);

            let stream_config = config.config();
            let sample_format = config.sample_format();

            let stream = build_input_stream(&device, &stream_config, sample_format, tx)
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            stream.play().map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            streams.push(stream);
            log::info!("Started capture: {} → channel {}", source.device_name, source.channel_id);
        }

        // For each output destination, start an output stream fed from a channel
        let mut output_senders: HashMap<String, Sender<AudioFrame>> = HashMap::new(); // bus → sender

        for output in &outputs {
            let device = match self.host.output_devices()
                .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
                .find(|d| d.name().map(|n| n == output.device_name).unwrap_or(false))
            {
                Some(d) => d,
                None => {
                    log::warn!("Output device '{}' not found, skipping", output.device_name);
                    continue;
                }
            };

            let config = device.default_output_config()
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            let stream_config = config.config();
            let sample_format = config.sample_format();

            let (tx, rx) = unbounded::<AudioFrame>();
            output_senders.insert(output.bus.clone(), tx);

            let stream = build_output_stream(&device, &stream_config, sample_format, rx)
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            stream.play().map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            streams.push(stream);
            log::info!("Started output: bus '{}' → {}", output.bus, output.device_name);
        }

        // Mix loop — runs in its own thread
        let mixer = self.mixer.clone();
        let running = self.running.clone();
        let mix_thread = thread::spawn(move || {
            const CHUNK: usize = 1024; // samples per channel per chunk
            while *running.read() {
                // Collect available frames from each capture channel
                let mut frames: HashMap<ChannelId, AudioFrame> = HashMap::new();
                for (ch_id, rx) in &capture_receivers {
                    let mut combined = Vec::new();
                    while let Ok(frame) = rx.try_recv() {
                        combined.extend_from_slice(&frame);
                        if combined.len() >= CHUNK { break; }
                    }
                    if !combined.is_empty() {
                        frames.insert(ch_id.clone(), combined);
                    }
                }

                if frames.is_empty() {
                    thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }

                // Mix into buses
                let mixed = mixer.mix(&frames, CHUNK);

                // Send each bus to its output
                for (bus, buf) in mixed {
                    if let Some(tx) = output_senders.get(&bus) {
                        let _ = tx.try_send(buf);
                    }
                }
            }
            log::info!("Mix loop stopped");
        });

        Ok(AudioEngineHandle {
            _streams: streams,
            _mix_thread: Some(mix_thread),
            running: self.running.clone(),
        })
    }

    pub fn stop(&self) {
        *self.running.write() = false;
    }
}

pub struct AudioEngineHandle {
    _streams: Vec<cpal::Stream>,
    _mix_thread: Option<thread::JoinHandle<()>>,
    running: Arc<parking_lot::RwLock<bool>>,
}

impl Drop for AudioEngineHandle {
    fn drop(&mut self) {
        *self.running.write() = false;
        if let Some(t) = self._mix_thread.take() {
            let _ = t.join();
        }
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    tx: Sender<AudioFrame>,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError> {
    match format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _| { let _ = tx.try_send(data.to_vec()); },
            |e| log::error!("Input stream error: {}", e),
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                let f32_data: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let _ = tx.try_send(f32_data);
            },
            |e| log::error!("Input stream error: {}", e),
            None,
        ),
        _ => device.build_input_stream(
            config,
            move |data: &[u8], _| {
                let f32_data: Vec<f32> = data.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect();
                let _ = tx.try_send(f32_data);
            },
            |e| log::error!("Input stream error: {}", e),
            None,
        ),
    }
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    rx: Receiver<AudioFrame>,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError> {
    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

    // Feed thread: pull from channel into buffer
    let buf_feed = buf.clone();
    let rx_clone = rx.clone();
    thread::spawn(move || {
        for frame in rx_clone {
            buf_feed.lock().extend_from_slice(&frame);
        }
    });

    match format {
        SampleFormat::F32 => {
            let buf_play = buf.clone();
            device.build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    let mut b = buf_play.lock();
                    let len = data.len().min(b.len());
                    data[..len].copy_from_slice(&b[..len]);
                    b.drain(..len);
                    for s in &mut data[len..] { *s = 0.0; }
                },
                |e| log::error!("Output stream error: {}", e),
                None,
            )
        }
        SampleFormat::I16 => {
            let buf_play = buf.clone();
            device.build_output_stream(
                config,
                move |data: &mut [i16], _| {
                    let mut b = buf_play.lock();
                    let len = data.len().min(b.len());
                    for (i, s) in data[..len].iter_mut().enumerate() {
                        *s = (b[i] * i16::MAX as f32) as i16;
                    }
                    b.drain(..len);
                    for s in &mut data[len..] { *s = 0; }
                },
                |e| log::error!("Output stream error: {}", e),
                None,
            )
        }
        _ => {
            let buf_play = buf.clone();
            device.build_output_stream(
                config,
                move |data: &mut [u8], _| {
                    let mut b = buf_play.lock();
                    let len = data.len().min(b.len());
                    for (i, s) in data[..len].iter_mut().enumerate() {
                        *s = ((b[i] * 128.0) + 128.0) as u8;
                    }
                    b.drain(..len);
                    for s in &mut data[len..] { *s = 128; }
                },
                |e| log::error!("Output stream error: {}", e),
                None,
            )
        }
    }
}
