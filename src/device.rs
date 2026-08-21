use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, SupportedStreamConfig};
use serde::{Deserialize, Serialize};
use crate::error::{AudioLibreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_default: bool,
}

pub struct DeviceManager {
    host: Host,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }

    pub fn list_inputs(&self) -> Result<Vec<AudioDevice>> {
        let default_input = self.host.default_input_device();
        let default_name = default_input
            .as_ref()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        self.host
            .input_devices()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
            .filter_map(|d| device_to_info(d, DeviceKind::Input, &default_name))
            .collect::<Vec<_>>()
            .into_iter()
            .map(Ok)
            .collect()
    }

    pub fn list_outputs(&self) -> Result<Vec<AudioDevice>> {
        let default_output = self.host.default_output_device();
        let default_name = default_output
            .as_ref()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        self.host
            .output_devices()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
            .filter_map(|d| device_to_info(d, DeviceKind::Output, &default_name))
            .collect::<Vec<_>>()
            .into_iter()
            .map(Ok)
            .collect()
    }

    pub fn default_input(&self) -> Result<AudioDevice> {
        let device = self
            .host
            .default_input_device()
            .ok_or_else(|| AudioLibreError::DeviceNotFound("No default input device".into()))?;
        let name = device
            .name()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?;
        Ok(AudioDevice {
            id: name.clone(),
            name: name.clone(),
            kind: DeviceKind::Input,
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
            is_default: true,
        })
    }

    pub fn default_output(&self) -> Result<AudioDevice> {
        let device = self
            .host
            .default_output_device()
            .ok_or_else(|| AudioLibreError::DeviceNotFound("No default output device".into()))?;
        let name = device
            .name()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?;
        let config = device
            .default_output_config()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?;
        Ok(AudioDevice {
            id: name.clone(),
            name: name.clone(),
            kind: DeviceKind::Output,
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
            is_default: true,
        })
    }

    pub fn find_input_by_name(&self, name: &str) -> Result<Device> {
        self.host
            .input_devices()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| AudioLibreError::DeviceNotFound(format!("Input '{}' not found", name)))
    }

    pub fn find_output_by_name(&self, name: &str) -> Result<Device> {
        self.host
            .output_devices()
            .map_err(|e| AudioLibreError::DeviceNotFound(e.to_string()))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| AudioLibreError::DeviceNotFound(format!("Output '{}' not found", name)))
    }
}

fn device_to_info(device: Device, kind: DeviceKind, default_name: &str) -> Option<AudioDevice> {
    let name = device.name().ok()?;
    let config: SupportedStreamConfig = match kind {
        DeviceKind::Input => device.default_input_config().ok()?,
        DeviceKind::Output => device.default_output_config().ok()?,
    };
    Some(AudioDevice {
        is_default: name == default_name,
        id: name.clone(),
        name,
        kind,
        sample_rate: config.sample_rate().0,
        channels: config.channels(),
    })
}
