# The patchplay MCP server

The MCP server lets an AI assistant build and change patches with the same commands the app and the
[`kerf` CLI](CLI.md) use. The server holds **one patch** in memory. The assistant reads it as `.kerf`
text ([KERF-FORMAT.md](KERF-FORMAT.md)), changes it with commands, and judges it with `check` and
`render`. It can also play it live on your speakers.

It is part of the `kerf` binary: there is nothing else to install.

## Start it

```sh
cargo build --release -p kerf-cli
target/release/kerf mcp [PATCH]
```

With a `PATCH` path (a `.kerf` file, such as one from `presets/`) the session starts from that file.
Without one it starts empty. The server never writes to the file; the assistant gets the patch back
as text, and saving it is up to you or the assistant.

You do not normally run this by hand: the MCP client starts it.

## Connect a client

Use the absolute path of the binary.

**Claude Code**

```sh
claude mcp add kerf -- /path/to/kerf-modular-playground/target/release/kerf mcp
```

To start from a preset, add its path after `mcp`:

```sh
claude mcp add kerf -- /path/to/kerf-modular-playground/target/release/kerf mcp /path/to/kerf-modular-playground/presets/first-cut.kerf
```

**Claude Desktop** (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "kerf": {
      "command": "/path/to/kerf-modular-playground/target/release/kerf",
      "args": ["mcp"]
    }
  }
}
```

Any other MCP client that can launch a stdio server works the same way: command `kerf`, argument
`mcp`.

## Protocol

- **Transport**: stdio. One JSON-RPC 2.0 message per line on stdin, one per line on stdout. Nothing
  but protocol messages is written to stdout. The server runs until stdin closes.
- **Protocol versions**: `2025-06-18`, `2025-03-26` and `2024-11-05`. If the client asks for one the
  server does not know, it answers with `2025-06-18`.
- **Methods**: `initialize`, `ping`, `tools/list` and `tools/call`. Notifications (such as
  `notifications/initialized`) are accepted and get no reply. `resources/list` and `prompts/list`
  return empty lists. Any other method gets error `-32601`.
- **Capabilities**: tools only. The resource and prompt lists are always empty.
- **Server info**: name `kerf`, the crate version, and a short `instructions` string telling the
  model how the tools fit together.
- **Errors**: a tool that fails returns a normal result with `isError: true` and the message as text,
  so the model can read it and try again. An unknown tool name, or a `tools/call` with no name, is a
  JSON-RPC error `-32602`. A line that is not JSON is `-32700`.
- **Arguments**: every tool rejects unknown argument names (`bad arguments: ...`). Tools with no
  arguments accept `{}` or none.
- **Results** are `content` arrays of `text` blocks. A "patch" in a result is `.kerf` text.

## Tools

Fifteen tools in all (`tools/list` returns exactly these). The tool descriptions the model sees
list every command, module kind and style, taken from the code so they cannot drift.

### Reading the patch

| Tool | Arguments | Returns |
|---|---|---|
| `patch_get` | none | The current patch as `.kerf` text. |
| `lineage` | none | JSON: `current` (node id) and `nodes`, each with `id`, `parents`, `origin` and `current` (bool). |
| `describe_module` | `kind`? (string) | With `kind`: JSON descriptor with `kind`, `band`, `hp`, `summary`, `params` (key, label, default, tooltip), `inputs` and `outputs` (jacks). Without: a JSON array of `{kind, band, summary}`. Unknown kind is an error. |
| `suggest_cables` | `kind`?, `module`?, `near`?, `part`? (all strings) | JSON `{"cables": [...]}`: the cables auto-patching would add, best first, each with `from`, `to`, `reason` and `score`. Does not change the patch. Give `module` (in the patch) or `kind` (not added yet). `near` limits it to cables between that module and `module`/`kind`; `part` is the part a new module would join. |
| `language_docs` | `section`? (string) | Without `section`: the list of section ids and titles. With one: that section's markdown (file format, worked examples, tips, module reference). |

### Changing the patch

| Tool | Arguments | Returns |
|---|---|---|
| `patch_apply` | `commands` (array of objects, required) | The new patch as `.kerf` text, then, if auto-patching added cables, a JSON block `{"connected": [{from, to, reason, score}, ...]}`. |
| `random` | `style` (string, required, one of the style names); `seed`? (integer ≥ 0) | A JSON line `{"node": id, "seed": n}`, then the new patch. |
| `evolve` | `strength`? (0 to 1, default 0.3); `seed`? | The same two blocks. |
| `breed` | `with` (required: integer lineage node id, or a patch as `.kerf` text); `seed`? | The same two blocks. |

`patch_apply` takes the commands documented in the
[CLI reference](CLI.md#command-reference): `add_part`, `add_module`, `auto_patch`, `move_module`,
`replace_text`, `remove_module`, `set_param`, `set_sample`, `connect`, `disconnect`, `lock`, `mute`,
`set_part_level`, `set_tempo`, `store_scene`, `clear_scene`, `morph`, `undo`, `redo`, `random`,
`evolve`, `breed` and `checkout`.

It is **all or none**: commands are applied to a copy, and if one fails the patch is unchanged and
the error names the command by its position: `command 0: no module `nope` in the patch; nothing was
applied`. If a patch is playing, the change is heard live.

`random`, `evolve` and `breed` each add a **lineage node** and make it current. When `seed` is
omitted the server uses the number of lineage nodes so far, so each call differs but the same
session history repeats; the reply's JSON line gives the seed used. Give a seed to control it. To go
back to an earlier node, use `patch_apply` with `{"op":"checkout","node":N}`.
To breed two earlier patches, check out the first, then `breed` with the second's node id. The
`lineage` root (node 0) is whatever the session started with.

`evolve` leaves locked things alone: lock them first with `patch_apply`
(`{"op":"lock","target":{"kind":"part","part":"bass"},"locked":true}`).

### Judging the patch

| Tool | Arguments | Returns |
|---|---|---|
| `check` | none | JSON report: `pass`, `seconds`, `sample_rate`, `peak_db`, `rms_db`, `ceiling_db`, `silent_fraction`, `dc_offset`, `finite`, `clipped`, `runaway`, `silent_kinds`, `problems`. Same check as `kerf check`: a 6 second render at 32 kHz. `pass` is false if any rule fails; `problems` says which. |
| `render` | `seconds`? (above 0, at most 120, default 8); `rate`? (8000 to 192000, default 48000); `path`? (string, must end in `.wav`) | JSON: `path`, `seconds`, `rate`, `peak_db`, `rms_db`, `silent_kinds`, `warnings`. Writes a 16-bit stereo WAV, atomically. Without `path` it goes to `kerf-render-<pid>-<n>.wav` in the temp folder. An existing file at `path` is overwritten. |

### Playing the patch

| Tool | Arguments | Returns |
|---|---|---|
| `play` | none | `playing at <rate> Hz; later changes are heard live`. Opens the default audio output on the first call. Fails with `no audio output: ...` if there is none. |
| `stop` | none | `stopped`. Does nothing if nothing is playing. |

## Safety

- `play` is the only tool that opens the audio device, and it is opened only when `play` is first
  called. A machine without audio can use every other tool. Playback does not stop when the
  conversation moves on: call `stop`, or end the server.
- `render` writes a file. The model chooses `path`, so it can overwrite any file the
  server's user can write. Without `path` it writes to the temp folder.
- `set_sample` stores a path to an audio file; the sampler reads it when the patch is rendered or
  played. A relative path, whether set by `set_sample` or written in the patch text, is taken from
  the folder of the file given to `kerf mcp`, as `kerf render` does. With no file (an empty or
  inline-only session) it is taken from the server's working directory, so use absolute paths there.
- The server reads no files unless started with a `PATCH` argument, or given a `set_sample` path.
  It never changes the file it started from.
- Nothing is sent over the network.

## A worked session

Each line below is one message. Requests from the client, shortened responses from the server
(`...` marks cut text). This is the real output of `kerf mcp`.

```
-> {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"demo","version":"0"}}}
<- {"id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"kerf","version":"0.1.0"},"instructions":"Kerf Modular is a patchable modular synth. ..."}}
-> {"jsonrpc":"2.0","method":"notifications/initialized"}

-> {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"random","arguments":{"style":"tape-keys","seed":4}}}
<- {"id":2,"result":{"content":[{"type":"text","text":"{\"node\":1,\"seed\":4}"},{"type":"text","text":"kerf 1\ntempo 89\nkey G major\nstyle tape-keys seed 4\n\nmodule tally1 Tally ..."}],"isError":false}}

-> {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"patch_apply","arguments":{"commands":[{"op":"set_tempo","bpm":84},{"op":"lock","target":{"kind":"part","part":"keys"},"locked":true}]}}}
<- {"id":3,"result":{"content":[{"type":"text","text":"kerf 1\ntempo 84\nkey G major\n...\npart keys \"Keys pad\" colour #7a4fb5 lock\n..."}],"isError":false}}

-> {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"evolve","arguments":{"strength":0.2,"seed":9}}}
<- {"id":4,"result":{"content":[{"type":"text","text":"{\"node\":3,\"seed\":9}"},{"type":"text","text":"kerf 1\ntempo 84\n..."}],"isError":false}}

-> {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"check","arguments":{}}}
<- {"id":5,"result":{"content":[{"type":"text","text":"{\n  \"pass\": true,\n  \"seconds\": 6.0,\n  ... \"problems\": []\n}"}],"isError":false}}

-> {"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"render","arguments":{"seconds":2,"path":"/tmp/kd/mcp.wav"}}}
<- {"id":6,"result":{"content":[{"type":"text","text":"{\n  \"path\": \"/tmp/kd/mcp.wav\",\n  \"peak_db\": -2.3999998569488525,\n  \"rate\": 48000,\n  \"rms_db\": -15.218982696533203,\n  \"seconds\": 2.0,\n  \"silent_kinds\": [],\n  \"warnings\": []\n}"}],"isError":false}}

-> {"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"patch_apply","arguments":{"commands":[{"op":"set_param","module":"nope","param":"x","value":1}]}}}
<- {"id":8,"result":{"content":[{"type":"text","text":"command 0: no module `nope` in the patch; nothing was applied"}],"isError":true}}
```

Note that the `patch_apply` in step 3 made lineage node 2 (an edit), so the `evolve` that followed
is node 3.

With a client such as Claude, you do not write these messages: you ask for "a slow, dusty patch",
and the model picks `random` with a style, `evolve`s it, locks the parts you like and `check`s the
result.

## Silent results

If `silent_kinds` in `check` or `render` is not empty, those module kinds are in the
patch but have no DSP yet, so they make no sound. A patch with nothing wired to the output renders
as silence: peak and RMS come out near -180 dBFS.
