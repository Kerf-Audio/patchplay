//! `kerf-app --serve`: the bridge over HTTP, for developing and testing the
//! web UI in a plain browser against the real session.
//!
//! Every Tauri command has an endpoint of the same name: `POST /api/<command>`
//! with the command's arguments as a JSON object (`invoke("apply", { cmd })`
//! is `POST /api/apply` with `{"cmd": …}`). Success is `200` with the result as
//! JSON; a rejected command is `400` with `{"error": "…"}`.
//!
//! These commands read and write files as the user, so the server is for this machine only,
//! and a web page in the user's browser must not be able to drive it: it binds a loopback
//! address only, answers only requests whose `Host` and `Origin` name this machine (a rebound
//! DNS name does not), and takes `POST` with `Content-Type: application/json` (a header a page
//! on another site cannot send without a preflight this server never grants). A client has
//! [`READ_TIMEOUT`] to send a whole request, so a stalled connection cannot hold a thread for
//! ever. Each connection takes one request.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use kerf_bridge::{args, json_of};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::bridge::{Bridge, BridgeError};

#[derive(Deserialize)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
struct SaveArgs {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
struct RenderArgs {
    path: String,
    seconds: f32,
}

/// Runs one command by name: the file commands here, the rest in the shared bridge. `None`
/// when no such command exists.
pub fn dispatch(bridge: &Bridge, name: &str, a: Value) -> Option<Result<Value, BridgeError>> {
    Some(match name {
        "open_file" => {
            args(a).and_then(|p: PathArgs| bridge.open_file(Path::new(&p.path)).map(json_of))
        }
        "save_file" => args(a).and_then(|p: SaveArgs| {
            bridge
                .save_file(p.path.as_deref().map(Path::new))
                .map(json_of)
        }),
        "render_wav" => args(a).and_then(|p: RenderArgs| {
            bridge
                .render_wav(Path::new(&p.path), p.seconds)
                .map(json_of)
        }),
        _ => return kerf_bridge::dispatch(bridge, name, a),
    })
}

/// The largest request body taken: a whole patch with its command is a few kilobytes.
const MAX_BODY: usize = 1 << 20;
/// The largest request head (request line and headers) taken.
const MAX_HEAD: usize = 16 << 10;
const MAX_HEADERS: usize = 64;
/// How long a client has to send a whole request, and to take the answer. Real requests arrive
/// in milliseconds.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A status and the reason to give with it.
type Refusal = (u16, &'static str);

const TIMED_OUT: Refusal = (408, "the request did not arrive in time");
const UNREADABLE: Refusal = (400, "the request could not be read");
const TOO_LARGE: Refusal = (431, "the request head is too large");

pub struct HttpServer {
    listener: TcpListener,
    bridge: Arc<Bridge>,
    pub(crate) read_timeout: Duration,
}

impl HttpServer {
    /// Binds `addr`, which must be a loopback address: the commands read and write files, so
    /// the server is for this machine only.
    pub fn bind(addr: &str, bridge: Arc<Bridge>) -> io::Result<Self> {
        let addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();
        if addrs.is_empty() || !addrs.iter().all(|a| a.ip().is_loopback()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{addr} is not a loopback address; the server is for this machine only"),
            ));
        }
        Ok(Self {
            listener: TcpListener::bind(&addrs[..])?,
            bridge,
            read_timeout: READ_TIMEOUT,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serves until the process ends.
    pub fn run(self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let bridge = Arc::clone(&self.bridge);
                    let timeout = self.read_timeout;
                    thread::spawn(move || handle(&bridge, stream, timeout));
                }
                Err(e) => eprintln!("accepting a connection: {e}"),
            }
        }
    }
}

/// The parts of a request the server looks at.
struct Head {
    method: String,
    url: String,
    host: Option<String>,
    origin: Option<String>,
    content_type: Option<String>,
    /// From `Content-Length`; a request without one has no body.
    content_length: usize,
}

/// Parses the head at the start of `buf`: the head and its length, or `None` while it is still
/// incomplete.
fn parse_head(buf: &[u8]) -> Result<Option<(Head, usize)>, Refusal> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut headers);
    let used = match parsed.parse(buf) {
        Ok(httparse::Status::Complete(used)) => used,
        Ok(httparse::Status::Partial) if buf.len() <= MAX_HEAD => return Ok(None),
        Ok(httparse::Status::Partial) => return Err(TOO_LARGE),
        Err(_) => return Err((400, "the request is not valid HTTP")),
    };
    if used > MAX_HEAD {
        return Err(TOO_LARGE);
    }
    let header = |name: &str| {
        parsed
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| String::from_utf8_lossy(h.value).trim().to_owned())
    };
    if header("Transfer-Encoding").is_some() {
        return Err((411, "send Content-Length; chunked bodies are not taken"));
    }
    let content_length = match header("Content-Length") {
        None => 0,
        Some(n) => n
            .parse()
            .map_err(|_| (400, "Content-Length is not a number"))?,
    };
    let head = Head {
        method: parsed.method.unwrap_or_default().to_owned(),
        url: parsed.path.unwrap_or_default().to_owned(),
        host: header("Host"),
        origin: header("Origin"),
        content_type: header("Content-Type"),
        content_length,
    };
    Ok(Some((head, used)))
}

/// Reads what the client has sent into `buf`, waiting no longer than `deadline`.
fn read_more(stream: &mut TcpStream, buf: &mut Vec<u8>, deadline: Instant) -> Result<(), Refusal> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(TIMED_OUT);
    }
    stream
        .set_read_timeout(Some(left))
        .map_err(|_| UNREADABLE)?;
    let mut chunk = [0; 4096];
    match stream.read(&mut chunk) {
        Ok(0) => Err(UNREADABLE),
        Ok(n) => {
            buf.extend_from_slice(&chunk[..n]);
            Ok(())
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Err(TIMED_OUT)
        }
        Err(_) => Err(UNREADABLE),
    }
}

/// Whether `authority` (`host` or `host:port`, as in a `Host` header or an origin) names this
/// machine.
fn is_loopback(authority: &str) -> bool {
    let (host, port) = match authority.strip_prefix('[') {
        Some(rest) => rest.split_once(']').unwrap_or((rest, "!")),
        None => authority.split_at(authority.find(':').unwrap_or(authority.len())),
    };
    let port_ok = port.is_empty()
        || port
            .strip_prefix(':')
            .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    port_ok
        && (host.eq_ignore_ascii_case("localhost")
            || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()))
}

/// The refusal for a request a page on another site could have sent, or `Ok` for one the UI
/// or a script on this machine would.
fn admit(head: &Head) -> Result<(), Refusal> {
    if !head.host.as_deref().is_some_and(is_loopback) {
        return Err((
            403,
            "Host must name this machine: localhost, 127.0.0.1 or [::1]",
        ));
    }
    let from_here = |origin: &str| {
        origin
            .split_once("://")
            .is_some_and(|(_, rest)| is_loopback(rest))
    };
    if head.origin.as_deref().is_some_and(|o| !from_here(o)) {
        return Err((403, "requests from other origins are refused"));
    }
    if head.method != "POST" {
        return Err((405, "use POST /api/<command>"));
    }
    let json = |value: &str| {
        value
            .split(';')
            .next()
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
    };
    if !head.content_type.as_deref().is_some_and(json) {
        return Err((415, "send Content-Type: application/json"));
    }
    if head.content_length > MAX_BODY {
        return Err((413, "request body too large"));
    }
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    }
}

fn reply(stream: &mut TcpStream, status: u16, payload: &Value) {
    let body = payload.to_string();
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reason(status),
        body.len()
    );
    // The client may have gone away; nothing to do about it.
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()));
}

/// Reads one request, admits it, and runs it.
fn serve_request(
    bridge: &Bridge,
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<(u16, Value), Refusal> {
    let mut buf = Vec::new();
    let (head, used) = loop {
        if let Some(found) = parse_head(&buf)? {
            break found;
        }
        read_more(stream, &mut buf, deadline)?;
    };
    admit(&head)?;
    let end = used + head.content_length;
    while buf.len() < end {
        read_more(stream, &mut buf, deadline)?;
    }
    let body =
        std::str::from_utf8(&buf[used..end]).map_err(|_| (400, "the body is not valid UTF-8"))?;
    Ok(route(bridge, &head.url, body))
}

fn handle(bridge: &Bridge, mut stream: TcpStream, timeout: Duration) {
    // A client that reads nothing must not hold the thread either.
    if stream.set_write_timeout(Some(timeout)).is_err() {
        return;
    }
    let (status, payload) = serve_request(bridge, &mut stream, Instant::now() + timeout)
        .unwrap_or_else(|(status, message)| (status, json!({ "error": message })));
    reply(&mut stream, status, &payload);
}

fn route(bridge: &Bridge, url: &str, body: &str) -> (u16, Value) {
    let Some(name) = url.strip_prefix("/api/") else {
        return (404, json!({ "error": format!("no such endpoint {url}") }));
    };
    let a = if body.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return (400, json!({ "error": format!("body is not JSON: {e}") })),
        }
    };
    match dispatch(bridge, name, a) {
        None => (404, json!({ "error": format!("no such command {name}") })),
        Some(Ok(v)) => (200, v),
        Some(Err(e)) => (400, json!({ "error": e.to_string() })),
    }
}
