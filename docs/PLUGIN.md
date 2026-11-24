# The patchplay plugin

patchplay builds as a CLAP and VST3 instrument (`crates/kerf-plugin`, using
[nih-plug](https://github.com/robbert-vdh/nih-plug)). The real engine runs on the host's audio thread, so
a patch sounds the same as in the app.

## Build and install

```sh
cargo xtask bundle kerf-plugin --release
```

This writes `target/bundled/patchplay.clap` and `target/bundled/patchplay.vst3`. On macOS they are
ad-hoc signed only, so a host may ask you to allow them. Copy them to:

| | CLAP | VST3 |
|---|---|---|
| macOS | `~/Library/Audio/Plug-Ins/CLAP/` | `~/Library/Audio/Plug-Ins/VST3/` |
| Linux | `~/.clap/` | `~/.vst3/` |
| Windows | `C:\Program Files\Common Files\CLAP\` | `C:\Program Files\Common Files\VST3\` |

Linux needs the GUI libraries listed in the README to build. `cargo xtask` is an alias set in
`.cargo/config.toml`; the bundle name comes from `bundler.toml`. nih-plug is pinned to one git revision in
`crates/kerf-plugin/Cargo.toml` and `xtask/Cargo.toml`; change both together.

## What it does

- **Patch in the project.** The patch is saved in the host's project as `.kerf` text (plus a name), so a
  project reopens with the patch it was saved with. A new instance starts with the `plain-sawn` starting
  point. State that does not open falls back to that too.
- **Starting points.** The editor lists the factory starting points, with a filter. Clicking one loads it.
  The text box names the patch. There is no file or code editing in the plugin; use the app or the web
  build for that.
- **Transport.** The patch plays while the host plays, and starts from the top each time the host
  starts. Turn off Follow transport to let it run all the time. The engine has no position control, so
  the plugin does not follow the host's bar position or loops; it follows play and stop.
- **Tempo.** With Follow host tempo on, the patch runs at the host's tempo (kept between 20 and 300 bpm).
  Without a host tempo, or with the switch off, the patch keeps its own.
- **Macros and level.** Four host parameters (Macro 1 to 4) drive the `m1` to `m4` knobs of the first Jig
  in the patch, and Level drives the volume of the first Finish. They are automatable. When you load a
  patch (a starting point, or the saved one when a project opens), the parameters move to the values the
  patch holds, so it sounds as its author left it; the patch plays with those values until you move a
  control. This means a project reopens with the patch's own values, not macro positions moved since. A patch with no Jig or no Finish
  ignores the matching parameters, and the editor says so.
- **Samples in the state.** The plugin reads local sample files, so a patch in the project state can name
  files on the machine (a `Sampler` path) and the plugin opens and decodes them when the project loads,
  in the background thread. They are only played, never sent anywhere, but open projects from people
  you trust as you would any project that references files.
- **Output.** Stereo out. No audio input, and no MIDI yet: the engine has no note or audio input module,
  so playing it from a keyboard needs a new module first.

## Real-time safety

The audio thread does no allocation, locking or file access. Patches are parsed, checked and compiled
off the audio thread (in the editor, or in nih-plug's background thread) and passed over lock-free
queues. A new patch fades in over 25 ms. Stopping fades out and builds a fresh engine in the background
for the next start. Retired engines go back over a queue to be dropped off the audio thread. The
`assert_process_allocs` feature turns on nih-plug's allocation check in debug builds:

```sh
cargo build -p kerf-plugin --features assert_process_allocs
```

## Tests

`cargo test -p kerf-plugin` has no host. It covers saved state, tempo mapping, the transport, the macros
and level, and that the output is finite and sounds at several sample rates and block sizes.
`clap-validator validate target/bundled/patchplay.clap` and `pluginval target/bundled/patchplay.vst3` check
the bundles against the formats.

## Known limits and next steps

- No MIDI or audio input (see above).
- The editor is a plain egui panel, not the web UI. Embedding the web UI would need a webview in the
  plugin window (for example with `nih_plug_webview`) and a transport for the bridge commands.
- Bar position and loop points from the host are not followed.
- No standalone build.
- Windows is not built or tested.
- nih-plug's CLAP wrapper reads a state length from the host and allocates it without a limit, so
  clap-validator's `state-invalid-random` test aborts. This is in nih-plug, not in patchplay.

## Licence

The VST3 bindings in nih-plug are GPLv3. AGPL-3.0-or-later may be combined with GPLv3, so the plugin is
distributed as AGPL-3.0-or-later. A CLAP-only build would not use them. Anyone distributing a VST3 plugin
also needs to meet Steinberg's trademark and licence terms.
