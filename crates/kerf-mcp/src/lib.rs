//! MCP server over stdio, newline-delimited JSON-RPC 2.0. An agent changes
//! the patch only through `Command`s applied by one `Session`, and reads it
//! back as `.kerf` text after each change.

mod handlers;
mod tools;

#[cfg(test)]
mod tests;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use kerf_modules::Builtin;
use kerf_session::Session;
use serde_json::{Value, json};

use handlers::Server;

/// Newest first; a client asking for one we do not know gets the first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const PARSE_ERROR: i32 = -32700;

/// A JSON-RPC error: code and message.
type RpcError = (i32, String);

/// Serves the session on stdin and stdout until stdin closes. `base` is the
/// folder of the `.kerf` file it was opened from, if any; relative sample
/// paths are taken from it.
pub fn serve_stdio(session: Session<Builtin>, base: Option<PathBuf>) -> io::Result<()> {
    serve(session, base, io::stdin().lock(), io::stdout().lock())
}

/// Serves one line-delimited JSON-RPC conversation. Nothing but protocol
/// messages is ever written to `output`.
pub fn serve(
    session: Session<Builtin>,
    base: Option<PathBuf>,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    let mut server = Server::new(session, base);
    // Bytes, not `lines()`: a line that is not UTF-8 is a parse error to answer, not the end of
    // the conversation.
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let line = line.trim_ascii();
        if line.is_empty() {
            continue;
        }
        if let Some(reply) = respond(&mut server, line) {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

fn respond(server: &mut Server, line: &[u8]) -> Option<Value> {
    let msg: Value = match serde_json::from_slice(line) {
        Ok(msg) => msg,
        Err(e) => {
            return Some(failure(
                &Value::Null,
                (PARSE_ERROR, format!("parse error: {e}")),
            ));
        }
    };
    let id = msg.get("id");
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        let is_response = msg.get("result").is_some() || msg.get("error").is_some();
        return (!is_response).then(|| {
            failure(
                id.unwrap_or(&Value::Null),
                (INVALID_REQUEST, "not a JSON-RPC request".into()),
            )
        });
    };
    // Notifications, such as `notifications/initialized`, get no reply.
    let id = id?;
    let params = msg.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => server.call(params),
        // Nothing here is offered as a resource or a prompt, but clients that ask get an answer.
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        _ => Err((METHOD_NOT_FOUND, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(e) => failure(id, e),
    })
}

fn failure(id: &Value, (code, message): RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let wanted = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .into_iter()
        .find(|v| Some(*v) == wanted)
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "kerf", "version": env!("CARGO_PKG_VERSION") },
        "instructions": tools::INSTRUCTIONS,
    })
}
