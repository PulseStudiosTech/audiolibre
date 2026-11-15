//! Device auto-detection: finds standalone microphones and the active output device.
//!
//! Strategy:
//! - Use cpal device names + heuristics (portable across all 3 platforms)
//! - Filter out headset mics, webcam mics, virtual devices
//! - On Windows: cross-reference with form factor via windows-rs (behind feature flag)

use crate::device::{AudioDevice, DeviceKind, DeviceManager};
use crate::error::{AudioLibreError, Result};

/// Confidence score for a detected device (higher = better match)
#[derive(Debug, Clone)]
pub struct DetectedDevice {
    pub device: AudioDevice,
    pub score: i32,
    pub reason: String,
}

/// Detect the best standalone microphone input device.
/// Excludes headset mics, webcam mics, and virtual devices.
/// Returns devices in descending order of confidence.
pub fn detect_microphones(manager: &DeviceManager) -> Result<Vec<DetectedDevice>> {
    let inputs = manager.list_inputs()?;
    let mut results: Vec<DetectedDevice> = Vec::new();

    for device in inputs {
        let (score, reason) = score_microphone(&device.name);
        if score > 0 {
            results.push(DetectedDevice { device, score, reason });
        }
    }

    results.sort_by(|a, b| b.score.cmp(&a.score));
    Ok(results)
}

/// Detect the current active output device (default output).
pub fn detect_active_output(manager: &DeviceManager) -> Result<AudioDevice> {
    manager.default_output()
}

/// Score a microphone by its device name.
/// Returns (score, reason). Score ≤ 0 means "exclude this device".
fn score_microphone(name: &str) -> (i32, String) {
    let lower = name.to_lowercase();

    // ── Hard excludes (score = -999) ─────────────────────────────────────────

    // Virtual/loopback devices
    if lower.contains("virtual") || lower.contains("loopback") ||
       lower.contains("vb-audio") || lower.contains("vb audio") ||
       lower.contains("audiolibre") || lower.contains("cable output") ||
       lower.contains("null") || lower.contains("monitor") {
        return (-999, "virtual/loopback device".into());
    }

    // Headset microphones — specifically excluded per product spec
    if lower.contains("headset") || lower.contains("headphone") ||
       lower.contains("earphone") || lower.contains("earpiece") ||
       lower.contains("airpods") || lower.contains("galaxy buds") ||
       lower.contains("jabra") || lower.contains("plantronics") ||
       lower.contains("poly ") || lower.contains("sennheiser") ||
       lower.contains("corsair hs") || lower.contains("astro a") ||
       lower.contains("steelseries arctis") || lower.contains("hyper x") ||
       lower.contains("hyperx") || lower.contains("razer kraken") ||
       lower.contains("razer blackshark") || lower.contains("turtle beach") {
        return (-999, "headset microphone excluded".into());
    }

    // Webcam microphones
    if lower.contains("webcam") || lower.contains("web cam") ||
       lower.contains("camera") || lower.contains("facetime") ||
       lower.contains("logitech c9") || lower.contains("brio") ||
       lower.contains("c920") || lower.contains("c922") ||
       lower.contains("streamcam") || lower.contains("integrated") {
        return (-50, "webcam/integrated microphone (low priority)".into());
    }

    // ── Positive signals (standalone mics) ───────────────────────────────────

    let mut score = 50i32; // default: unknown input device gets a baseline
    let mut reasons: Vec<&str> = Vec::new();

    // Professional/prosumer XLR/USB mics
    if lower.contains("blue yeti") || lower.contains("blue snowball") ||
       lower.contains("blue ember") || lower.contains("blue spark") {
        score += 200; reasons.push("Blue Designs USB mic");
    }
    if lower.contains("shure sm") || lower.contains("shure mv") ||
       lower.contains("shure beta") {
        score += 200; reasons.push("Shure microphone");
    }
    if lower.contains("rode nt") || lower.contains("rode podmic") ||
       lower.contains("rode procaster") || lower.contains("rode ai") {
        score += 200; reasons.push("RODE microphone");
    }
    if lower.contains("elgato wave") {
        score += 150; reasons.push("Elgato Wave USB mic");
    }
    if lower.contains("audio-technica") || lower.contains("at2020") ||
       lower.contains("at2035") || lower.contains("atr") {
        score += 180; reasons.push("Audio-Technica microphone");
    }
    if lower.contains("hyperx quadcast") || lower.contains("hyperx solocast") {
        score += 150; reasons.push("HyperX USB mic (standalone)");
    }
    if lower.contains("razer seiren") {
        score += 150; reasons.push("Razer Seiren USB mic");
    }
    if lower.contains("samson") || lower.contains("fifine") ||
       lower.contains("tonor") || lower.contains("maono") {
        score += 100; reasons.push("USB condenser mic");
    }
    // Generic USB microphone signals
    if lower.contains("usb mic") || lower.contains("usb microphone") ||
       lower.contains("condenser") || lower.contains("xlr") {
        score += 80; reasons.push("USB/condenser mic");
    }
    // Named "microphone" without headset/webcam markers — good candidate
    if lower.contains("microphone") && score == 50 {
        score += 30; reasons.push("named microphone device");
    }
    // Windows: "Microphone Array" (usually laptop built-in, low priority)
    if lower.contains("array") {
        score -= 30; reasons.push("microphone array (likely built-in)");
    }
    // macOS built-in mic
    if lower.contains("built-in") || lower.contains("internal") {
        score -= 30; reasons.push("built-in microphone");
    }

    let reason = if reasons.is_empty() {
        format!("input device '{}' (no specific signals)", name)
    } else {
        reasons.join(", ")
    };

    (score, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yeti_high_score() {
        let (score, _) = score_microphone("Blue Yeti USB Microphone");
        assert!(score > 200, "Blue Yeti should score high");
    }

    #[test]
    fn test_headset_excluded() {
        let (score, _) = score_microphone("Headset Microphone (Corsair HS60)");
        assert!(score < 0, "Headset mics should be excluded");
    }

    #[test]
    fn test_webcam_low_score() {
        let (score, _) = score_microphone("Logitech C920 Microphone");
        assert!(score < 0, "Webcam mic should score low or be excluded");
    }

    #[test]
    fn test_virtual_excluded() {
        let (score, _) = score_microphone("VB-Audio Virtual Cable");
        assert!(score < 0, "Virtual device should be excluded");
    }

    #[test]
    fn test_shure_high_score() {
        let (score, _) = score_microphone("Shure MV7 USB Microphone");
        assert!(score > 150, "Shure mic should score high");
    }
}
