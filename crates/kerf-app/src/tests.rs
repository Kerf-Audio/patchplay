//! The desktop's own share: files, the HTTP server and the audio thread's seams. The commands
//! themselves are tested in `kerf-bridge`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use kerf_bridge::presets;
use kerf_core::Command;
use kerf_engine::Render;
use serde_json::{Value, json};

use crate::bridge::{Bridge, BridgeError};
use crate::serve::{HttpServer, dispatch};
use crate::transport::NoAudio;

fn bridge() -> Bridge {
    Bridge::new(Box::new(NoAudio("no sound card".into())))
}

/// A scratch folder per test, removed at the end of the test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kerf-app-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn steel_shop_text() -> &'static str {
    presets::text("steel-shop").unwrap()
}

#[test]
fn factory_presets_open_by_their_preset_path_and_never_save_over_themselves() {
    let b = bridge();
    let found = b.list_presets();
    let state = b.open_file(std::path::Path::new(&found[0].path)).unwrap();
    assert!(!state.patch.modules.is_empty());
    assert_eq!(
        state.path, None,
        "a factory preset opens as an unsaved session"
    );
    assert!(b.save_file(None).is_err(), "Save must ask for a file");
    assert!(b.open_file(std::path::Path::new("preset:nothing")).is_err());
}

#[test]
fn save_writes_the_opened_file_or_a_new_one() {
    let dir = Dir::new("save");
    let (copy, other) = (dir.file("copy.kerf"), dir.file("other.kerf"));
    std::fs::write(&copy, steel_shop_text()).unwrap();

    let b = bridge();
    b.open_file(&copy).unwrap();
    b.apply(Command::SetTempo { bpm: 101.0 }).unwrap();
    b.save_file(None).unwrap();
    assert!(
        std::fs::read_to_string(&copy)
            .unwrap()
            .contains("tempo 101")
    );

    let state = b.save_file(Some(&other)).unwrap();
    assert_eq!(
        state.path.as_deref(),
        Some(other.display().to_string().as_str())
    );
    let state = bridge().open_file(&other).unwrap();
    assert_eq!(state.patch, b.get_state().patch);
}

#[test]
fn saving_with_no_file_asks_for_one() {
    let err = bridge().save_file(None).unwrap_err();
    assert!(matches!(err, BridgeError::Invalid(_)));
}

#[test]
fn save_adds_kerf_to_a_bare_name_and_refuses_any_other_extension() {
    let dir = Dir::new("save-extension");
    let b = bridge();
    b.open_preset("steel-shop").unwrap();

    let state = b.save_file(Some(&dir.file("bare"))).unwrap();
    let saved = dir.file("bare.kerf");
    assert_eq!(
        state.path.as_deref(),
        Some(saved.display().to_string().as_str())
    );
    assert!(saved.exists() && !dir.file("bare").exists());
    b.save_file(Some(&dir.file("LOUD.KERF"))).unwrap();
    let leftovers = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
        .count();
    assert_eq!(leftovers, 0, "a save left its temporary file behind");

    let err = b.save_file(Some(&dir.file("notes.txt"))).unwrap_err();
    assert!(err.to_string().contains("must end in .kerf"), "{err}");
    assert!(!dir.file("notes.txt").exists());

    // A file opened under another name is not overwritten by a plain Save either.
    let profile = dir.file("profile.sh");
    std::fs::write(&profile, steel_shop_text()).unwrap();
    let b = bridge();
    b.open_file(&profile).unwrap();
    assert!(b.save_file(None).is_err());
    assert_eq!(
        std::fs::read_to_string(&profile).unwrap(),
        steel_shop_text()
    );
}

#[test]
fn a_bad_file_names_itself_in_the_error() {
    let dir = Dir::new("missing");
    let err = bridge().open_file(&dir.file("missing.kerf")).unwrap_err();
    assert!(err.to_string().contains("missing.kerf"), "{err}");
}

#[test]
fn render_writes_a_wav_that_is_audible() {
    let dir = Dir::new("render");
    let b = bridge();
    b.open_preset("steel-shop").unwrap();
    let out = dir.file("out.wav");
    let r = b.render_wav(&out, 2.0).unwrap();
    assert!(r.peak_db > -40.0 && r.peak_db < 0.5, "peak {}", r.peak_db);
    assert_eq!(r.path, out.display().to_string());
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(bytes.len(), 44 + 2 * 48_000 * 4);
    assert!(b.render_wav(&out, 9999.0).is_err());
}

#[test]
fn render_adds_wav_to_a_bare_name_and_refuses_any_other_extension() {
    let dir = Dir::new("render-extension");
    let b = bridge();
    b.open_preset("steel-shop").unwrap();

    let r = b.render_wav(&dir.file("bare"), 0.1).unwrap();
    assert_eq!(r.path, dir.file("bare.wav").display().to_string());
    assert!(dir.file("bare.wav").exists() && !dir.file("bare").exists());
    for name in ["out.mp3", "patch.kerf", "profile.sh"] {
        let err = b.render_wav(&dir.file(name), 0.1).unwrap_err();
        assert!(err.to_string().contains("must end in .wav"), "{err}");
        assert!(!dir.file(name).exists(), "{name} was written");
    }
}

#[test]
fn a_sample_path_in_a_kerf_file_is_relative_to_the_file() {
    let dir = Dir::new("sample");
    let tone: Vec<f32> = (0..48_000)
        .map(|n| 0.3 * (std::f32::consts::TAU * 440.0 * n as f32 / 48_000.0).sin())
        .collect();
    Render {
        left: tone.clone(),
        right: tone,
    }
    .write_wav(&dir.file("loop.wav"), 48_000)
    .unwrap();
    std::fs::write(
        dir.file("song.kerf"),
        "kerf 1\nmodule v1 Veneer sample=\"loop.wav\"\nmodule out Finish\ncable v1.l -> out.l\n",
    )
    .unwrap();
    let b = bridge();
    b.open_file(&dir.file("song.kerf")).unwrap();

    let p = b.sample_peaks("v1").unwrap();
    assert_eq!(
        (p.file.as_deref(), p.warning.as_deref()),
        (Some("loop.wav"), None)
    );
    assert!((p.seconds - 1.0).abs() < 0.001);

    let state = b
        .apply(Command::SetSample {
            module: "v1".parse().unwrap(),
            path: Some("gone.wav".into()),
        })
        .unwrap();
    assert_eq!(state.patch.modules[0].sample.as_deref(), Some("gone.wav"));
    let p = b.sample_peaks("v1").unwrap();
    assert!(p.file.is_none() && p.warning.as_deref().unwrap().contains("not found"));
}

#[test]
fn dispatch_opens_files_and_passes_the_rest_to_the_shared_bridge() {
    let dir = Dir::new("dispatch");
    std::fs::write(dir.file("a.kerf"), steel_shop_text()).unwrap();
    let b = bridge();
    dispatch(
        &b,
        "open_file",
        json!({"path": dir.file("a.kerf").display().to_string()}),
    )
    .unwrap()
    .unwrap();
    let state = dispatch(&b, "apply", json!({"cmd": {"op":"set_tempo","bpm":99}}))
        .unwrap()
        .unwrap();
    assert_eq!(state["patch"]["tempo"], 99.0);
    assert!(dispatch(&b, "nope", json!({})).is_none());
}

/// A raw exchange over a fresh connection: the status and the body of whatever comes back.
fn exchange(addr: SocketAddr, request: &str) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let status = String::from_utf8_lossy(&raw[..split])
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, raw[split..].to_vec())
}

/// The headers of a request from this machine's own UI or a script.
const LOCAL: &str = "Host: 127.0.0.1\r\nContent-Type: application/json\r\n";

/// A request with exactly these header lines (each ending `\r\n`), and the JSON that comes back.
fn request(addr: SocketAddr, method: &str, path: &str, headers: &str, body: &str) -> (u16, Value) {
    let length = body.len();
    let (status, body) = exchange(
        addr,
        &format!(
            "{method} {path} HTTP/1.1\r\n{headers}Content-Length: {length}\r\nConnection: close\r\n\r\n{body}"
        ),
    );
    (status, serde_json::from_slice(&body).unwrap())
}

fn http(addr: SocketAddr, path: &str, body: &str) -> (u16, Value) {
    request(addr, "POST", path, LOCAL, body)
}

/// A server on a free loopback port, with a bridge that has no sound card.
fn serve() -> SocketAddr {
    spawn(HttpServer::bind("127.0.0.1:0", Arc::new(bridge())).unwrap())
}

fn spawn(server: HttpServer) -> SocketAddr {
    let addr = server.local_addr().unwrap();
    thread::spawn(move || server.run());
    addr
}

#[test]
fn http_endpoints_mirror_the_commands() {
    let addr = serve();

    let (status, state) = http(addr, "/api/open_file", r#"{"path":"preset:steel-shop"}"#);
    assert_eq!(status, 200);
    assert_eq!(state["patch"]["tempo"], 138.0);

    let set = r#"{"cmd":{"op":"set_tempo","bpm":100}}"#;
    let (status, state) = http(addr, "/api/apply", set);
    assert_eq!(
        (status, state["patch"]["tempo"].clone()),
        (200, json!(100.0))
    );

    let (status, err) = http(addr, "/api/apply", r#"{"cmd":{"op":"set_tempo","bpm":1}}"#);
    assert_eq!(status, 400);
    assert!(err["error"].as_str().unwrap().contains("tempo"), "{err}");

    assert_eq!(http(addr, "/api/nothing", "{}").0, 404);
    assert_eq!(http(addr, "/api/meter", "").1["peak_l"], 0.0);
    assert_eq!(http(addr, "/api/scope", "").1["samples"], json!([]));
}

#[test]
fn http_refuses_what_a_web_page_on_another_site_could_send() {
    let addr = serve();
    let dir = Dir::new("cross-site");
    let out = dir.file("out.wav");
    let body = json!({ "path": out, "seconds": 0.1 }).to_string();
    let render = |headers: &str| request(addr, "POST", "/api/render_wav", headers, &body).0;

    // A form or a `fetch` without a preflight can only send these content types.
    for headers in [
        "Host: 127.0.0.1\r\nContent-Type: text/plain\r\n",
        "Host: 127.0.0.1\r\nContent-Type: application/x-www-form-urlencoded\r\n",
        "Host: 127.0.0.1\r\nContent-Type: multipart/form-data; boundary=x\r\n",
        "Host: 127.0.0.1\r\n",
    ] {
        assert_eq!(render(headers), 415, "{headers}");
    }
    // DNS rebinding: the page's own name now resolves to this machine, and says so in `Host`.
    for host in [
        "evil.example",
        "evil.example:8787",
        "127.0.0.1.evil.example",
        "localhost.evil.example",
        "127.0.0.1@evil.example",
        "127.0.0.1:80x",
        "10.0.0.5:8787",
        "[::1",
    ] {
        let headers = format!("Host: {host}\r\nContent-Type: application/json\r\n");
        assert_eq!(render(&headers), 403, "{host}");
    }
    // A page allowed to send JSON still names the site it came from.
    for origin in ["http://evil.example", "https://evil.example:8787", "null"] {
        let headers = format!("{LOCAL}Origin: {origin}\r\n");
        assert_eq!(render(&headers), 403, "{origin}");
    }
    // Nothing but `POST` runs a command.
    for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
        let (status, _) = request(addr, method, "/api/render_wav", LOCAL, &body);
        assert_eq!(status, 405, "{method}");
    }
    assert!(!out.exists(), "a refused request still wrote a file");

    // The UI through Vite's proxy, and a script on this machine, are served.
    for headers in [
        "Host: localhost:5173\r\nOrigin: http://localhost:5173\r\nContent-Type: application/json\r\n",
        "Host: [::1]:8787\r\nContent-Type: application/json; charset=utf-8\r\n",
        "Host: LOCALHOST\r\nContent-Type: Application/JSON\r\n",
    ] {
        std::fs::remove_file(&out).ok();
        assert_eq!(render(headers), 200, "{headers}");
        assert!(out.exists(), "{headers}");
    }
}

#[test]
fn http_refuses_a_body_beyond_any_patch_without_reading_it() {
    let addr = serve();
    // No body follows: the declared length alone is refused.
    let (status, body) = exchange(
        addr,
        &format!(
            "POST /api/apply HTTP/1.1\r\n{LOCAL}Content-Length: 2097152\r\nConnection: close\r\n\r\n"
        ),
    );
    assert_eq!(status, 413);
    assert!(String::from_utf8_lossy(&body).contains("too large"));
}

/// A server that gives a client `timeout` to send a whole request.
fn serve_within(timeout: Duration) -> SocketAddr {
    let mut server = HttpServer::bind("127.0.0.1:0", Arc::new(bridge())).unwrap();
    server.read_timeout = timeout;
    spawn(server)
}

#[test]
fn http_gives_up_on_a_client_that_stalls_part_way_through_a_request() {
    let addr = serve_within(Duration::from_millis(300));
    let head = format!("POST /api/apply HTTP/1.1\r\n{LOCAL}");
    // The head never ends; the body stops short of its declared length.
    for partial in [
        head.clone(),
        format!("{head}Content-Length: 100\r\n\r\n{{\"cmd\""),
    ] {
        let (status, body) = exchange(addr, &partial);
        assert_eq!(status, 408, "{partial}");
        assert!(String::from_utf8_lossy(&body).contains("in time"));
    }

    // A client that keeps trickling bytes has one deadline for the whole request, not one per read.
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut reader = stream.try_clone().unwrap();
    let started = Instant::now();
    let answer = thread::spawn(move || {
        let mut raw = String::new();
        reader.read_to_string(&mut raw).unwrap();
        (raw, started.elapsed())
    });
    stream.write_all(b"POST /api/apply HTTP/1.1\r\n").unwrap();
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if stream.write_all(b"X-Slow: 1\r\n").is_err() {
            break;
        }
    }
    let (raw, took) = answer.join().unwrap();
    assert!(raw.starts_with("HTTP/1.1 408"), "{raw}");
    assert!(took < Duration::from_secs(2), "answered after {took:?}");
}

#[test]
fn http_takes_a_request_sent_in_pieces_and_refuses_a_malformed_one() {
    let addr = serve();
    let body = r#"{"cmd":{"op":"set_tempo","bpm":100}}"#;
    let (first, rest) = body.split_at(10);
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST /api/apply HTTP/1.1\r\n{LOCAL}Content-Length: {}\r\nConnection: close\r\n\r\n{first}",
        body.len()
    )
    .unwrap();
    thread::sleep(Duration::from_millis(50));
    stream.write_all(rest.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    assert!(raw.starts_with("HTTP/1.1 200 OK"), "{raw}");
    assert!(raw.contains(r#""tempo":100"#), "{raw}");

    let long = "x".repeat(17_000);
    for (request, expected) in [
        ("garbage\r\n\r\n".to_string(), 400),
        (
            format!("POST /api/apply HTTP/1.1\r\n{LOCAL}Content-Length: many\r\n\r\n"),
            400,
        ),
        (
            format!("POST /api/apply HTTP/1.1\r\n{LOCAL}Transfer-Encoding: chunked\r\n\r\n"),
            411,
        ),
        (
            format!("POST /api/apply HTTP/1.1\r\n{LOCAL}X-Long: {long}\r\n\r\n"),
            431,
        ),
    ] {
        assert_eq!(exchange(addr, &request).0, expected, "{request:.80}");
    }
}

#[test]
fn the_server_binds_loopback_addresses_only() {
    for addr in ["0.0.0.0:0", "192.168.1.20:8787", "[::]:0"] {
        let Err(refused) = HttpServer::bind(addr, Arc::new(bridge())) else {
            panic!("{addr} was bound");
        };
        assert_eq!(refused.kind(), std::io::ErrorKind::InvalidInput, "{addr}");
        assert!(refused.to_string().contains("loopback"), "{refused}");
    }
    assert!(HttpServer::bind("localhost:0", Arc::new(bridge())).is_ok());
}
