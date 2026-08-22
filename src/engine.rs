//! AudioEngine v2 — dropout-resistant, low-latency monitoring, device hot-swap ready.
//!
//! Three improvements over v1:
//!  1. Lock-free ring buffers for output (no Mutex in audio callbacks → no glitches)
//!  2. Direct monitoring path: mic → headphones at 256-sample chunks (~5ms) bypassing mix thread
//!  3. MIX_CHUNK reduced 1024→256 for tighter mix loop timing

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use crossbeam_channel::{bounded, Sender, Receiver};
use parking_lot::RwLock;
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Observer, Split};
use crate::Mixer;
use crate::mixer::ChannelId;
use crate::error::{AudioLibreError, Result};

pub type AudioFrame = Vec<f32>;

/// Mix thread chunk size — 256 samples ≈ 5.3ms at 48kHz (down from 1024/21ms)
const MIX_CHUNK: usize = 256;

/// Output ring buffer capacity — 16384 samples ≈ 341ms at 48kHz
/// Large enough to absorb scheduling jitter without overflow
const RING_CAPACITY: usize = 16384;

/// Monitoring path bounded channel depth — 4 frames max in flight
const MONITOR_CHAN_DEPTH: usize = 4;

/// One capture source → one mixer channel
#[derive(Debug, Clone)]
pub struct CaptureSource {
    pub device_name: String,
    pub channel_id: ChannelId,
    /// Route this source directly to the monitoring output (low-latency path)
    pub monitor: bool,
}

/// One output bus → one hardware device
#[derive(Debug, Clone)]
pub struct OutputDest {
    pub device_name: String,
    pub bus: String,
    /// This device receives the direct monitoring signal instead of the mix bus
    pub is_monitor_out: bool,
}

pub struct AudioEngine {
    mixer: Arc<Mixer>,
}

impl AudioEngine {
    pub fn new(mixer: Arc<Mixer>) -> Self {
        Self { mixer }
    }

    pub fn start(
        &self,
        sources: Vec<CaptureSource>,
        outputs: Vec<OutputDest>,
    ) -> Result<AudioEngineHandle> {
        let host = cpal::default_host();
        let mut streams: Vec<cpal::Stream> = Vec::new();

        // ── Monitoring channel (lock-free bounded, non-blocking try_send/try_recv) ──
        let (monitor_tx, monitor_rx) = bounded::<AudioFrame>(MONITOR_CHAN_DEPTH);

        // ── Output ring buffers (one per bus, HeapRb = lock-free SPSC) ────────────
        // Build a ring buffer for each output bus; consumers go to audio callbacks.
        let mut bus_ring_prods: HashMap<String, ringbuf::HeapProd<f32>> = HashMap::new();
        let mut bus_ring_cons: HashMap<String, ringbuf::HeapCons<f32>> = HashMap::new();
        for out in &outputs {
            if !out.is_monitor_out {
                let rb = HeapRb::<f32>::new(RING_CAPACITY);
                let (prod, cons) = rb.split();
                bus_ring_prods.insert(out.bus.clone(), prod);
                bus_ring_cons.insert(out.bus.clone(), cons);
            }
        }

        // Monitoring ring buffer: capture callbacks → crossbeam channel → drain thread → ring → output
        let monitor_ring_rb = HeapRb::<f32>::new(RING_CAPACITY);
        let (mut monitor_ring_prod, monitor_ring_cons) = monitor_ring_rb.split();
        // Wrap in Option so it can be moved into the output loop at most once
        let mut monitor_ring_cons_opt: Option<ringbuf::HeapCons<f32>> = Some(monitor_ring_cons);

        // Drain thread: monitor_rx → monitor_ring_prod
        let mon_rx_for_drain = monitor_rx;
        thread::Builder::new()
            .name("al-monitor-drain".into())
            .spawn(move || {
                for frame in mon_rx_for_drain {
                    let avail = monitor_ring_prod.vacant_len();
                    let n = frame.len().min(avail);
                    monitor_ring_prod.push_slice(&frame[..n]);
                }
            })
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        // ── Build output streams ─────────────────────────────────────────────────
        for out in &outputs {
            let device = match host.output_devices()
                .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
                .find(|d| d.name().map(|n| n == out.device_name).unwrap_or(false))
            {
                Some(d) => d,
                None => {
                    log::warn!("Output device '{}' not found, skipping", out.device_name);
                    continue;
                }
            };

            let cfg = device.default_output_config()
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

            if out.is_monitor_out {
                if let Some(cons) = monitor_ring_cons_opt.take() {
                let stream = build_ring_output(&device, &cfg.config(), cfg.sample_format(), cons)
                    .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
                stream.play().map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
                streams.push(stream);
                log::info!("Monitor output → {}", out.device_name);
                }
            } else {
                let cons = match bus_ring_cons.remove(&out.bus) {
                    Some(c) => c,
                    None => { log::warn!("No ring consumer for bus '{}'", out.bus); continue; }
                };
                let stream = build_ring_output(&device, &cfg.config(), cfg.sample_format(), cons)
                    .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
                stream.play().map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
                streams.push(stream);
                log::info!("Bus '{}' output → {}", out.bus, out.device_name);
            }
        }

        // ── Build capture streams ────────────────────────────────────────────────
        let mut capture_senders: HashMap<ChannelId, Sender<AudioFrame>> = HashMap::new();
        let mut capture_receivers: HashMap<ChannelId, Receiver<AudioFrame>> = HashMap::new();

        for src in &sources {
            let device = match host.input_devices()
                .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
                .find(|d| d.name().map(|n| n == src.device_name).unwrap_or(false))
            {
                Some(d) => d,
                None => {
                    log::warn!("Input device '{}' not found, skipping", src.device_name);
                    continue;
                }
            };

            let cfg = device.default_input_config()
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

            // Bounded channel — provides natural backpressure; 8 chunks of buffer
            let (tx, rx) = bounded::<AudioFrame>(8);
            capture_senders.insert(src.channel_id.clone(), tx.clone());
            capture_receivers.insert(src.channel_id.clone(), rx);

            let mon_tx = if src.monitor { Some(monitor_tx.clone()) } else { None };

            let stream = build_input_stream(&device, &cfg.config(), cfg.sample_format(), tx, mon_tx)
                .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            stream.play().map_err(|e| AudioLibreError::StreamError(e.to_string()))?;
            log::info!("Capture '{}' → ch {} (monitor={})", src.device_name, src.channel_id, src.monitor);
            streams.push(stream);
        }

        // ── Mix thread ────────────────────────────────────────────────────────────
        let running = Arc::new(RwLock::new(true));
        let running_mix = running.clone();
        let mixer = self.mixer.clone();

        let mix_thread = thread::Builder::new()
            .name("al-mix".into())
            .spawn(move || {
                while *running_mix.read() {
                    let mut frames: HashMap<ChannelId, AudioFrame> = HashMap::new();
                    let mut any = false;

                    for (ch_id, rx) in &capture_receivers {
                        let mut buf: Vec<f32> = Vec::with_capacity(MIX_CHUNK * 2);
                        while buf.len() < MIX_CHUNK {
                            match rx.try_recv() {
                                Ok(frame) => { buf.extend_from_slice(&frame); any = true; }
                                Err(_) => break,
                            }
                        }
                        if !buf.is_empty() {
                            frames.insert(ch_id.clone(), buf);
                        }
                    }

                    if !any {
                        // Sleep shorter than before (500µs vs 1ms) to reduce mix latency
                        thread::sleep(Duration::from_micros(500));
                        continue;
                    }

                    let mixed = mixer.mix(&frames, MIX_CHUNK);

                    for (bus, buf) in mixed {
                        if let Some(prod) = bus_ring_prods.get_mut(&bus) {
                            let avail = prod.vacant_len();
                            let n = buf.len().min(avail);
                            prod.push_slice(&buf[..n]);
                        }
                    }
                }
                log::info!("Mix thread exited");
            })
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        Ok(AudioEngineHandle {
            _streams: streams,
            _mix_thread: Some(mix_thread),
            running,
        })
    }
}

pub struct AudioEngineHandle {
    pub _streams: Vec<cpal::Stream>,
    pub _mix_thread: Option<thread::JoinHandle<()>>,
    pub running: Arc<RwLock<bool>>,
}

impl Drop for AudioEngineHandle {
    fn drop(&mut self) {
        *self.running.write() = false;
        if let Some(t) = self._mix_thread.take() {
            let _ = t.join();
        }
    }
}

// ── Stream builders ────────────────────────────────────────────────────────────

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    tx: Sender<AudioFrame>,
    monitor_tx: Option<Sender<AudioFrame>>,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError> {
    macro_rules! input_cb {
        ($conv:expr) => {{
            let tx = tx.clone();
            let mon = monitor_tx.clone();
            move |data: &[_], _: &cpal::InputCallbackInfo| {
                let samples: AudioFrame = data.iter().map($conv).collect();
                if let Some(ref m) = mon {
                    let _ = m.try_send(samples.clone());
                }
                let _ = tx.try_send(samples);
            }
        }};
    }

    match format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            input_cb!(|&s: &f32| s),
            |e| log::error!("Input error: {e}"),
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            input_cb!(|&s: &i16| s as f32 / i16::MAX as f32),
            |e| log::error!("Input error: {e}"),
            None,
        ),
        _ => device.build_input_stream(
            config,
            input_cb!(|&s: &u8| (s as f32 - 128.0) / 128.0),
            |e| log::error!("Input error: {e}"),
            None,
        ),
    }
}

fn build_ring_output(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    mut cons: ringbuf::HeapCons<f32>,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError> {
    match format {
        SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let n = cons.pop_slice(data);
                for s in &mut data[n..] { *s = 0.0; }
            },
            |e| log::error!("Output error: {e}"),
            None,
        ),
        SampleFormat::I16 => {
            let mut tmp = vec![0.0f32; 8192];
            device.build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let need = data.len();
                    if tmp.len() < need { tmp.resize(need, 0.0); }
                    let n = cons.pop_slice(&mut tmp[..need]);
                    for (i, s) in data.iter_mut().enumerate() {
                        *s = if i < n { (tmp[i] * i16::MAX as f32) as i16 } else { 0 };
                    }
                },
                |e| log::error!("Output error: {e}"),
                None,
            )
        }
        _ => device.build_output_stream(
            config,
            move |data: &mut [u8], _: &cpal::OutputCallbackInfo| {
                for s in data.iter_mut() { *s = 128; }
            },
            |e| log::error!("Output error: {e}"),
            None,
        ),
    }
}
