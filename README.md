# AudioLibre

The open-source Windows kernel driver and cross-platform Rust audio engine
behind [Prismix](https://pulsestudios.cc), a free, Wave Link-style virtual
audio mixer by [Pulse Studios](https://pulsestudios.cc).

AudioLibre creates real virtual audio devices and a software mixing engine
that routes hardware inputs/outputs into them, letting streamers and
creators build custom audio mixes (game audio, mic, music, voice chat,
etc.) without a dedicated hardware mixer or a third-party driver dependency
like VB-Audio.

This repository holds the open-source core: the kernel driver and the
engine that talks to it. The Prismix desktop app (GUI, mixer profiles,
virtual-device management, plugin ecosystem) is a separate, closed-source
application built on top of this.

## Structure

- **`driver/`** — Windows kernel driver (WaveRT miniport under PortCls,
  based on Microsoft's SYSVAD reference design). Exposes virtual render/
  capture devices that any application can select as an audio device.
- **`engine/`** — Rust, built on [`cpal`](https://github.com/RustAudio/cpal)
  for cross-platform audio I/O (WASAPI on Windows, Core Audio on macOS,
  ALSA/PipeWire on Linux). Device enumeration, capture/mix/output, a
  software mixer, and an IPC protocol (named pipe on Windows, Unix socket
  elsewhere) that a GUI or other tooling can drive.

## Building

Driver: Visual Studio 2022 with the Windows Driver Kit (WDK) for Windows
11 build 26100. See `driver/audiolibre.inf` and `driver/audiolibre.vcxproj`.

Engine: standard Rust toolchain, `cargo build` from `engine/`.

## License

MIT — see [LICENSE](LICENSE). If you use AudioLibre's driver or engine in
your own project, please keep the copyright notice intact; a credit back
to Pulse Studios is appreciated.
