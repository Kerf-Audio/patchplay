//! Protocol tests over in-memory pipes. The real-process test lives in kerf-cli.

use std::io::Cursor;
use std::path::PathBuf;

use kerf_core::text::parse;
use kerf_modules::Builtin;
use kerf_session::Session;
use serde_json::{Value, json};

use crate::serve;

const FIRST_CUT: &str = include_str!("../../../presets/first-cut.kerf");

fn session() -> Session<Builtin> {
    Session::open(Builtin, parse(FIRST_CUT).unwrap()).unwrap()
}

/// Sends the lines and returns every reply, in order.
fn talk(session: Session<Builtin>, lines: &[String]) -> Vec<Value> {
    talk_in(session, None, lines)
}

/// As `talk`, for a session opened from a file in the folder `base`.
fn talk_in(session: Session<Builtin>, base: Option<PathBuf>, lines: &[String]) -> Vec<Value> {
    let input = lines.join("\n");
    let mut out = Vec::new();
    serve(session, base, Cursor::new(input), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn request(id: u32, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn call(id: u32, tool: &str, arguments: Value) -> String {
    request(
        id,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    )
}

fn blocks(reply: &Value) -> Vec<&str> {
    reply["result"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["text"].as_str().unwrap())
        .collect()
}

fn is_error(reply: &Value) -> bool {
    reply["result"]["isError"].as_bool().unwrap()
}

#[test]
fn initialize_announces_tools_and_echoes_a_known_version() {
    let replies = talk(
        session(),
        &[
            request(1, "initialize", json!({ "protocolVersion": "2024-11-05" })),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
            request(2, "initialize", json!({ "protocolVersion": "1999-01-01" })),
        ],
    );
    assert_eq!(replies.len(), 2, "the notification gets no reply");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(replies[1]["result"]["protocolVersion"], "2025-06-18");
    assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "kerf");
}

#[test]
fn lists_every_tool_with_a_schema() {
    let replies = talk(session(), &[request(1, "tools/list", Value::Null)]);
    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "patch_get",
            "patch_apply",
            "random",
            "evolve",
            "breed",
            "lineage",
            "render",
            "check",
            "describe_module",
            "suggest_cables",
            "language_docs",
            "play",
            "stop"
        ]
    );
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(!tool["description"].as_str().unwrap().is_empty());
    }
    let random = tools.iter().find(|t| t["name"] == "random").unwrap();
    assert!(
        !random["inputSchema"]["properties"]["style"]["enum"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn patching_tools_report_cables_and_teach_the_format() {
    let replies = talk(
        session(),
        &[
            call(
                1,
                "patch_apply",
                json!({ "commands": [{ "op": "add_module", "kind": "Mortise", "id": "kick2", "auto_patch": true }] }),
            ),
            call(2, "suggest_cables", json!({ "kind": "Bandsaw" })),
            call(3, "language_docs", json!({})),
            call(4, "language_docs", json!({ "section": "cables" })),
            call(5, "language_docs", json!({ "section": "nope" })),
        ],
    );
    assert!(!is_error(&replies[0]));
    assert!(blocks(&replies[0])[0].contains("kick2"));
    assert!(
        blocks(&replies[0])
            .get(1)
            .is_none_or(|b| b.contains("\"connected\""))
    );
    assert!(!is_error(&replies[1]));
    assert!(blocks(&replies[1])[0].contains("\"cables\""));
    assert!(blocks(&replies[2])[0].contains("file-anatomy"));
    assert!(blocks(&replies[3])[0].starts_with("# Cables"));
    assert!(is_error(&replies[4]));
}

#[test]
fn patch_apply_changes_the_patch_all_or_nothing() {
    let set = |value: f32| json!({ "op": "set_param", "module": "flt1", "param": "cutoff", "value": value });
    let replies = talk(
        session(),
        &[
            call(1, "patch_apply", json!({ "commands": [set(0.9)] })),
            call(
                2,
                "patch_apply",
                json!({ "commands": [set(0.1), { "op": "remove_module", "id": "nope" }] }),
            ),
            call(
                3,
                "patch_apply",
                json!({ "commands": [{ "op": "set_tempo" }] }),
            ),
            call(4, "patch_get", json!({})),
        ],
    );
    assert!(!is_error(&replies[0]));
    assert!(blocks(&replies[0])[0].contains("cutoff=0.9"));
    assert!(is_error(&replies[1]));
    assert!(blocks(&replies[1])[0].starts_with("command 1: "));
    assert!(is_error(&replies[2]));
    assert!(blocks(&replies[2])[0].starts_with("command 0 is not a command"));
    let after = blocks(&replies[3])[0];
    assert!(
        after.contains("cutoff=0.9"),
        "the failed batches changed nothing"
    );
}

#[test]
fn render_writes_a_wav_and_reports_levels() {
    let path = std::env::temp_dir().join(format!("kerf-mcp-test-{}.wav", std::process::id()));
    let replies = talk(
        session(),
        &[call(
            1,
            "render",
            json!({ "seconds": 0.5, "rate": 16000, "path": path }),
        )],
    );
    assert!(!is_error(&replies[0]), "{:?}", blocks(&replies[0]));
    let stats: Value = serde_json::from_str(blocks(&replies[0])[0]).unwrap();
    assert_eq!(stats["rate"], 16000);
    assert!(stats["peak_db"].as_f64().unwrap() > -60.0, "{stats}");
    let wav = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(wav.len(), 44 + 8000 * 4);
}

/// A mono 16-bit WAV of a 220 Hz sine, `seconds` long.
fn sine_wav(seconds: f32) -> Vec<u8> {
    let rate = 22_050_u32;
    let samples: Vec<i16> = (0..(seconds * rate as f32) as usize)
        .map(|i| {
            let t = i as f32 / rate as f32;
            ((t * 220.0 * std::f32::consts::TAU).sin() * 20_000.0) as i16
        })
        .collect();
    let data = samples.len() as u32 * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

/// Renders a patch whose only sound is a Veneer playing `tone.wav`, named by a
/// relative path, and returns the render's report.
fn render_report(base: Option<PathBuf>, folder: &std::path::Path) -> Value {
    let out = folder.join("out.wav");
    let text = "kerf 1\ntempo 120\n\nmodule clk Tally\nmodule v1 Veneer sample=\"tone.wav\"\n\
                module out Finish\n\ncable clk.clock -> v1.trig\ncable v1.l -> out.l\n\
                cable v1.r -> out.r\n";
    let session = Session::open(Builtin, parse(text).unwrap()).unwrap();
    let replies = talk_in(
        session,
        base,
        &[call(
            1,
            "render",
            json!({ "seconds": 1, "rate": 22050, "path": out }),
        )],
    );
    assert!(!is_error(&replies[0]), "{:?}", blocks(&replies[0]));
    serde_json::from_str(blocks(&replies[0])[0]).unwrap()
}

#[test]
fn relative_sample_paths_resolve_against_the_patch_folder() {
    let folder = std::env::temp_dir().join(format!("kerf-mcp-samples-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("tone.wav"), sine_wav(1.0)).unwrap();

    let found = render_report(Some(folder.clone()), &folder);
    let lost = render_report(None, &folder);
    std::fs::remove_dir_all(&folder).unwrap();

    assert_eq!(found["warnings"], json!([]), "{found}");
    assert!(found["peak_db"].as_f64().unwrap() > -30.0, "{found}");
    // With no file there is no folder: the path is taken from the working
    // directory, where there is no tone.wav.
    assert!(!lost["warnings"].as_array().unwrap().is_empty(), "{lost}");
}

#[test]
fn render_rejects_silly_arguments() {
    let replies = talk(
        session(),
        &[
            call(1, "render", json!({ "seconds": 0 })),
            call(2, "render", json!({ "rate": 5 })),
            call(3, "render", json!({ "secs": 1 })),
        ],
    );
    assert!(replies.iter().all(is_error));
}

#[test]
fn render_writes_only_wav_files() {
    let path = std::env::temp_dir().join(format!("kerf-mcp-ext-{}.sh", std::process::id()));
    let replies = talk(
        session(),
        &[call(
            1,
            "render",
            json!({ "seconds": 0.1, "path": path.display().to_string() }),
        )],
    );
    assert!(is_error(&replies[0]));
    assert!(blocks(&replies[0])[0].contains("must end in .wav"));
    assert!(!path.exists(), "a render was written to a .sh file");
}

#[test]
fn resources_and_prompts_are_listed_as_empty_for_clients_that_ask() {
    let replies = talk(
        session(),
        &[
            request(1, "resources/list", Value::Null),
            request(2, "resources/templates/list", Value::Null),
            request(3, "prompts/list", Value::Null),
        ],
    );
    assert_eq!(replies[0]["result"], json!({ "resources": [] }));
    assert_eq!(replies[1]["result"], json!({ "resourceTemplates": [] }));
    assert_eq!(replies[2]["result"], json!({ "prompts": [] }));
}

#[test]
fn generators_record_lineage_and_can_be_checked_out() {
    let style = kerf_gen::styles().remove(0).name;
    let replies = talk(
        session(),
        &[
            call(1, "random", json!({ "style": style, "seed": 3 })),
            call(2, "evolve", json!({ "strength": 0.2, "seed": 1 })),
            call(3, "lineage", json!({})),
            call(
                4,
                "patch_apply",
                json!({ "commands": [{ "op": "checkout", "node": 0 }] }),
            ),
            call(5, "check", json!({})),
            call(6, "describe_module", json!({ "kind": "Plane" })),
            call(7, "describe_module", json!({ "kind": "Nonesuch" })),
        ],
    );
    for reply in &replies[..6] {
        assert!(!is_error(reply), "{:?}", blocks(reply));
    }
    let made: Value = serde_json::from_str(blocks(&replies[0])[0]).unwrap();
    assert_eq!(made["node"], 1);
    assert!(parse(blocks(&replies[0])[1]).is_ok());
    let lineage: Value = serde_json::from_str(blocks(&replies[2])[0]).unwrap();
    assert_eq!(lineage["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(lineage["current"], 2);
    assert_eq!(
        blocks(&replies[3])[0],
        kerf_core::text::print(session().patch())
    );
    let report: Value = serde_json::from_str(blocks(&replies[4])[0]).unwrap();
    assert!(report["pass"].is_boolean());
    let plane: Value = serde_json::from_str(blocks(&replies[5])[0]).unwrap();
    assert_eq!(plane["kind"], "Plane");
    assert!(is_error(&replies[6]));
}

#[test]
fn protocol_errors_are_answered_not_fatal() {
    let replies = talk(
        session(),
        &[
            "{not json".to_string(),
            request(1, "nonsense", Value::Null),
            call(2, "no_such_tool", json!({})),
            request(3, "ping", Value::Null),
        ],
    );
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["error"]["code"], -32601);
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["result"], json!({}));
}

#[test]
fn a_line_that_is_not_utf8_is_a_parse_error_not_the_end_of_the_conversation() {
    let mut input = b"\xff\xfe{\"jsonrpc\"\n".to_vec();
    input.extend_from_slice(request(7, "ping", Value::Null).as_bytes());
    input.extend_from_slice(b"\r\n");
    let mut out = Vec::new();
    serve(session(), None, Cursor::new(input), &mut out).unwrap();
    let replies: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(
        (replies[1]["id"].clone(), replies[1]["result"].clone()),
        (json!(7), json!({}))
    );
}
