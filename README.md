# SoundGIF

SoundGIF stores compressed audio inside a valid GIF file. Normal GIF viewers ignore the audio
data and display the animation. SoundGIF-aware apps and plugins find the embedded data, verify it,
and play it with the animation.

The project contains:

- A Rust desktop app for creating and playing SoundGIF files.
- A Rust command-line tool for embedding, extracting, inspecting, and removing audio.
- A Vencord plugin that plays SoundGIF attachments inside Discord.

## Repository layout

```text
src/                         Rust library, CLI, desktop app, and UI
tests/                       CLI integration tests
plugins/vencord/soundGif/    Discord/Vencord plugin
```

Other social-platform integrations can be added under `plugins/` without coupling them to the
encoder.

## Desktop app

The desktop app accepts:

- MP4, M4V, MOV, WebM, MKV, and AVI files for conversion.
- Existing GIF files for SoundGIF playback and inspection.

Conversion uses FFmpeg. SoundGIF first tries to copy compatible audio without re-encoding it. If
that is not possible, it falls back to variable-bitrate Opus. GIF frames use a generated palette,
rectangle-difference updates, and configurable size and frame-rate limits.

Build both executables:

```sh
cargo build --release --bins
```

The results are:

```text
target/release/soundgif
target/release/soundgif-player
```

Windows adds the `.exe` suffix.

### Platform requirements

- Windows: Microsoft Edge WebView2 Runtime and FFmpeg.
- macOS: the built-in WKWebView and FFmpeg.
- Linux: WebKitGTK 4.1, GTK 3, and FFmpeg.

SoundGIF checks for an `ffmpeg` executable beside the app first, then checks `PATH`. On Windows the
sibling executable is named `ffmpeg.exe`.

Ubuntu/Debian build dependencies:

```sh
sudo apt install libwebkit2gtk-4.1-dev
```

The CLI and file format do not depend on a desktop webview.

Release assets include Windows x64, a universal macOS build for Apple Silicon and Intel, and Linux
x64. The macOS app is not code-signed, so the first launch may require approval in **System
Settings > Privacy & Security**.

## CLI

Create a SoundGIF from video:

```sh
soundgif from-video clip.mp4 -o clip.sound.gif
```

Embed an existing audio stream:

```sh
soundgif embed animation.gif sound.opus -o animation-with-sound.gif
```

Inspect, extract, or remove embedded audio:

```sh
soundgif inspect animation-with-sound.gif
soundgif extract animation-with-sound.gif -o recovered.opus
soundgif strip animation-with-sound.gif -o animation-silent.gif
```

Run `soundgif help` for conversion and playback options.

## Vencord plugin

The Vencord package includes rerunnable installers:

- Windows: double-click `install-windows.cmd`.
- macOS: double-click `install-macos.command`.
- Linux: run `bash install-linux.sh`.

Each installer is one standalone file. Each run updates the SoundGIF and Vencord source checkouts, restores
`Vencord/src/userplugins/soundGif`, rebuilds Vencord, and runs Vencord's injector. This also
repairs the custom build if a later Vencord update replaces it. Restart Discord and enable
**SoundGIF** in Vencord settings afterward.

The installer menu can enable a per-user automatic check. It runs at sign-in and every 15 minutes,
rebuilds when SoundGIF or Vencord changed, and repairs the selected Stable, PTB, or Canary Discord
install if its Vencord patch is gone. The same menu can disable and remove the scheduled check.

Git, Node.js, and pnpm are required because Vencord does not load external user plugins without
being built from source.

The plugin detects the SoundGIF application block inside the file. It does not depend on a special
filename. It keeps GIF and audio loops on one timeline, pauses when the GIF is not playing, applies
safe default volume and peak limiting, and provides a per-attachment mute control.

The plugin treats embedded data only as media. It rejects oversized files, reads network responses
with a hard byte limit, verifies the payload structure and CRC-32, accepts only known audio MIME
types, and passes the result to Chromium's media decoder. It does not evaluate embedded code.

## File format

Audio is stored before the GIF trailer in a GIF89a Application Extension.

| Field | Value |
| --- | --- |
| Application identifier | `SNDGIF01` |
| Authentication code | `001` |
| Payload magic | `SGA1` |
| Payload version | `1` |

The payload contains playback flags, audio start offset, byte length, CRC-32, MIME type, original
filename, and compressed audio bytes. It is split into standard GIF data sub-blocks of at most 255
bytes.

Services that resize or re-encode GIF uploads can remove the application extension. Direct file
uploads work only when the service preserves the original GIF bytes.

## Development

Run the local checks:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

The GitHub Actions workflow checks Windows, macOS, and Linux builds.

## Licenses

The Rust app and SoundGIF format implementation are MIT licensed. The Vencord plugin is
GPL-3.0-or-later because it is built against Vencord. Packaged FFmpeg binaries keep their own
license and source-offer requirements.
