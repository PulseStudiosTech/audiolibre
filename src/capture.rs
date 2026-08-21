use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::error::{AudioLibreError, Result};

pub type AudioFrame = Vec<f32>;

/// Captures audio from a single input device and sends frames to a channel.
pub struct InputCapture {
    _stream: Stream,
    pub receiver: Receiver<AudioFrame>,
}

impl InputCapture {
    pub fn start(device: &Device, config: &StreamConfig) -> Result<Self> {
        let (tx, rx) = bounded::<AudioFrame>(128);
        let _err_tx = tx.clone();

        let stream = device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    let _ = tx.try_send(data.to_vec());
                },
                move |err| {
                    log::error!("Input stream error: {}", err);
                },
                None,
            )
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            receiver: rx,
        })
    }
}

/// Plays audio to a single output device, consuming frames from a channel.
pub struct OutputPlayer {
    _stream: Stream,
    pub sender: Sender<AudioFrame>,
    pub buffer: Arc<Mutex<Vec<f32>>>,
}

impl OutputPlayer {
    pub fn start(device: &Device, config: &StreamConfig) -> Result<Self> {
        let (tx, rx) = bounded::<AudioFrame>(128);
        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buf_clone = buffer.clone();

        // Feed incoming frames into a playback buffer
        std::thread::spawn(move || {
            for frame in rx {
                let mut b = buf_clone.lock();
                b.extend_from_slice(&frame);
            }
        });

        let play_buf = buffer.clone();
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    let mut b = play_buf.lock();
                    let len = data.len().min(b.len());
                    data[..len].copy_from_slice(&b[..len]);
                    b.drain(..len);
                    // Silence remaining if buffer ran dry
                    for s in &mut data[len..] { *s = 0.0; }
                },
                move |err| {
                    log::error!("Output stream error: {}", err);
                },
                None,
            )
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioLibreError::StreamError(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            sender: tx,
            buffer,
        })
    }
}
