//! `kerf mcp` as a real child process, spoken to in JSON-RPC over its pipes.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Client {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
}

impl Client {
    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], id, "{reply}");
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    /// Calls a tool and returns its text blocks.
    fn tool(&mut self, name: &str, arguments: Value) -> (bool, Vec<String>) {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let blocks = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["text"].as_str().unwrap().to_owned())
            .collect();
        (result["isError"].as_bool().unwrap(), blocks)
    }
}

#[test]
fn an_agent_can_read_change_and_render_a_patch() {
    let preset = concat!(env!("CARGO_MANIFEST_DIR"), "/../../presets/first-cut.kerf");
    let mut child = Command::new(env!("CARGO_BIN_EXE_kerf"))
        .args(["mcp", preset])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut client = Client {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        next_id: 0,
    };

    let init = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" },
        }),
    );
    assert_eq!(init["serverInfo"]["name"], "kerf");
    client.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let listed = client.request("tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for wanted in ["patch_get", "patch_apply", "render", "check", "lineage"] {
        assert!(names.contains(&wanted), "{names:?}");
    }

    let (failed, got) = client.tool("patch_get", json!({}));
    assert!(!failed);
    assert!(got[0].contains("cutoff=0.32"), "{}", got[0]);

    let set = json!({ "op": "set_param", "module": "flt1", "param": "cutoff", "value": 0.77 });
    let (failed, applied) = client.tool("patch_apply", json!({ "commands": [set] }));
    assert!(!failed, "{applied:?}");
    assert!(applied[0].contains("cutoff=0.77"), "{}", applied[0]);

    let wav = std::env::temp_dir().join(format!("kerf-cli-mcp-{}.wav", std::process::id()));
    let (failed, rendered) = client.tool(
        "render",
        json!({ "seconds": 1, "rate": 24000, "path": wav }),
    );
    assert!(!failed, "{rendered:?}");
    let stats: Value = serde_json::from_str(&rendered[0]).unwrap();
    assert!(stats["peak_db"].as_f64().unwrap() > -60.0, "{stats}");
    let bytes = std::fs::read(&wav).unwrap();
    std::fs::remove_file(&wav).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(bytes.len(), 44 + 24_000 * 4);

    drop(client);
    assert!(
        child.wait().unwrap().success(),
        "closing stdin ends the server"
    );
}
