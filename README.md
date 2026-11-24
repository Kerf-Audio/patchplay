<p align="center"><img src="ui/public/icon-512.png" alt="patchplay icon" width="128" height="128"></p>

<h1 align="center">patchplay</h1>

A modular audio playground by [Kerf Audio](https://kerfaudio.com). Patch 37 modules together, or let it
patch them for you, then roll random patches in a style, evolve them and breed two together. The sound
is generated as you listen; nothing is a loop or a recording.

It runs in the browser, from the same code that builds the desktop app.

## Use it

- **In the browser:** [patchplay.kerfaudio.com](https://patchplay.kerfaudio.com). Nothing to install.
- **Your own copy:** each [release](https://github.com/Kerf-Audio/patchplay/releases) has the web build as a
  zip to serve from the root of any site. From source, run `scripts/build-web.sh`, then serve
  `ui/dist-web` with any static file server, for example `python3 -m http.server -d ui/dist-web 8765`.
- **Desktop app:** coming later. Until then, build it yourself as below.
- **As a plugin:** patchplay is a CLAP and VST3 instrument that plays its patch in time with your host.
  Build it with `cargo xtask bundle kerf-plugin --release` (see below), then copy the bundles to the
  plugin folder for your system, and rescan in your host:

  | | CLAP (`patchplay.clap`) | VST3 (`patchplay.vst3`) |
  |---|---|---|
  | macOS | `~/Library/Audio/Plug-Ins/CLAP/` | `~/Library/Audio/Plug-Ins/VST3/` |
  | Linux | `~/.clap/` | `~/.vst3/` |
  | Windows | `C:\Program Files\Common Files\CLAP\` | `C:\Program Files\Common Files\VST3\` |

  The bundles are not signed or notarised. It has no MIDI input yet, and the macros and level are
  host parameters. Details and limits are in [docs/PLUGIN.md](docs/PLUGIN.md). Windows is not tested.

To build the desktop app on Linux you need the Tauri system libraries:
`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev patchelf`.
On macOS the Xcode command-line tools are enough. The source is at
[github.com/Kerf-Audio/patchplay](https://github.com/Kerf-Audio/patchplay).

## What is in it

- **37 modules**, named after joinery and woodwork, in six bands. Time: clocks, Euclidean rhythms,
  sequencers, a drawn-note pattern, a quantiser, a chord maker and an arpeggiator. Motion: LFOs, a step
  modulator, random, envelopes, a spring and pitch drift. Voices: oscillators, a dub siren, a detuned
  radio, wavetable, modal and drum voices, a sampler and slicer, and a granular sampler. Shape: filters,
  drive, compression and an FM, AM and ring modulator. Space: delay, a dub echo and reverb. Out: a mixer
  and a master stage.
- **Starting points** to open and change: four-four, abstract and crazy. Each has a Play button to hear
  it first; Open takes it to the workbench.
- **Random, evolve and breed** within six styles, with a lineage tree to go back to any earlier patch.
- **Beginner mode**: drag a module over another and it shows the cables it would make, then wires them
  when you drop it.
- **Code view**: every patch is plain `.kerf` text you can edit, with completion, live errors and a
  guide to the language. See [docs/KERF-FORMAT.md](docs/KERF-FORMAT.md).
- **Scenes and morph**, part levels, locks, undo, WAV export, and a small oscilloscope and spectrum
  analyser for the master output in the footer.
- **Desktop extras**: a `kerf` command-line tool and an MCP server, so an AI assistant can build and
  change patches with the same commands the app uses.

## Building

You need Rust (stable) and Node 22 or later.

```sh
# desktop app, run without bundling
cd ui && npm ci && npm run build && cd ..
cargo run -p kerf-app

# desktop app, bundled (needs `cargo install tauri-cli --version "^2" --locked` once)
cd ui && npm ci && cd ../crates/kerf-app
cargo tauri build            # bundles land in target/release/bundle/

# web app (static site in ui/dist-web)
scripts/build-web.sh

# command-line tool
cargo build --release -p kerf-cli
target/release/kerf --help
```

```sh
# CLAP and VST3 plugin: target/bundled/patchplay.clap and patchplay.vst3
cargo xtask bundle kerf-plugin --release
```

On Linux the plugin editor needs `libasound2-dev libgl-dev libjack-dev libx11-xcb-dev libxcb1-dev
libxcb-dri2-0-dev libxcb-icccm4-dev libxcursor-dev libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev`.

The command-line tool is documented in [docs/CLI.md](docs/CLI.md). `kerf mcp` runs the MCP server
that lets an AI assistant build and change patches; see [docs/MCP.md](docs/MCP.md) for the tools and
how to connect Claude Code or Claude Desktop.

To write patches by hand in the code editor, start with [docs/WRITING-PATCHES.md](docs/WRITING-PATCHES.md);
the [snippet library](docs/SNIPPETS.md) has drums, bass, leads, pads, chords and melodies, percussion,
modulation and effects to paste in (⌘K in the
editor lists them all).

Developing the UI with hot reload:

```sh
cargo run -p kerf-app -- --serve 127.0.0.1:8787   # backend (each request has a 10 s deadline)
cd ui && npm run dev                                # http://127.0.0.1:5173
```

Tests: `cargo test --workspace` and `cd ui && npm test`.

## Layout

| Path | What |
|---|---|
| `crates/kerf-core` | Patch model, commands, `.kerf` text. No I/O. |
| `crates/kerf-dsp`, `crates/kerf-modules` | DSP building blocks and the 37 modules. |
| `crates/kerf-engine` | Turns a patch into sound. |
| `crates/kerf-session` | Undo, scenes, lineage. |
| `crates/kerf-gen` | Styles and the random, evolve and breed generators. |
| `crates/kerf-app` | Desktop app (Tauri) and the development server. |
| `crates/kerf-cli`, `crates/kerf-mcp` | Command-line tool and MCP server. |
| `ui/` | The interface (TypeScript, Vite). |
| `crates/kerf-plugin`, `xtask` | The CLAP and VST3 plugin and its bundler. See [docs/PLUGIN.md](docs/PLUGIN.md). |
| `presets/` | Starting points. |

## Licence

[AGPL-3.0-or-later](LICENSE). You may use, study, change and share patchplay. If you distribute it, or run a
changed version for others over a network, you must share your source under the same licence.

Patches you make with patchplay, and audio you render with it, are yours.

The CLAP and VST3 plugin builds on the [nih-plug](https://github.com/robbert-vdh/nih-plug) framework. The
VST3 format is reached through bindings that are licensed GPLv3, which AGPL-3.0-or-later may be combined
with, so the plugin as a whole is distributed under AGPL-3.0-or-later. The CLAP build has no such
dependency. Steinberg's VST3 trademark and licensing terms apply to anyone who distributes a VST3 plugin.
