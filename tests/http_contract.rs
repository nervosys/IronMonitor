//! The HTTP surface an external program reads when it runs `ironmon serve`.
//!
//! These drive the built binary over real sockets, with `--port 0`, and read the port
//! from the one line it announces on stdout. A library test would pass while the
//! binary's argv, its announce line, its exit codes or its key handling were broken.
//!
//! Where an answer depends on this machine (whether an id has a value here), the
//! expected status is taken from `ironmon get` run on the same machine, so the test
//! asserts agreement rather than a particular platform.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A scratch directory for this test's key and stderr files, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("ironmon-http-contract-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).expect("create scratch directory");
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The binary, with provider keys removed the way `tests/agentic_contract.rs` removes
/// them, so a test never calls a real host with a developer's credentials.
fn ironmon() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironmon"));
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "IRONMON_OPENAI_ADMIN_KEY",
        "IRONMON_ANTHROPIC_ADMIN_KEY",
        "IRONMON_API_KEY",
    ] {
        command.env_remove(key);
    }
    command
}

/// A running `ironmon serve` on an OS-chosen port.
struct Server {
    child: Child,
    port: u16,
    announced: String,
    /// Kept open so the server's stdout pipe never closes under it.
    _stdout: BufReader<ChildStdout>,
    stderr_path: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn stderr(&self) -> String {
        fs::read_to_string(&self.stderr_path).unwrap_or_default()
    }
}

/// Start `ironmon serve --port 0 <extra>` and wait for its announce line.
fn serve(scratch: &Scratch, extra: &[&str]) -> Server {
    let stderr_path = scratch.path("stderr.txt");
    let stderr = File::create(&stderr_path).expect("stderr file");
    let mut args = vec!["serve", "--port", "0"];
    args.extend_from_slice(extra);
    let mut child = ironmon()
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn ironmon serve");
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(stdout);
    let mut announced = String::new();
    reader
        .read_line(&mut announced)
        .expect("read announce line");
    let port = parse_announce(&announced);
    Server {
        child,
        port,
        announced,
        _stdout: reader,
        stderr_path,
    }
}

/// The announce line must be exactly `{"listening":"<addr>"}` plus a newline.
fn parse_announce(line: &str) -> u16 {
    assert!(
        line.starts_with("{\"listening\":\"") && line.ends_with("\"}\n"),
        "first stdout line is not the announce line: {line:?}"
    );
    let addr = &line["{\"listening\":\"".len()..line.len() - "\"}\n".len()];
    let (host, port) = addr.rsplit_once(':').expect("address has a port");
    assert!(!host.is_empty(), "announce carries no host");
    let port: u16 = port.parse().expect("announced port is a number");
    assert_ne!(port, 0, "the announced port is the one really bound");
    port
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == &name.to_ascii_lowercase())
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.body))
    }
}

/// Send one request and read the whole response. `headers` may override `Host`.
fn request(port: u16, method: &str, path: &str, headers: &[(&str, &str)]) -> Response {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(600)))
        .expect("read timeout");
    let host_overridden = headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host"));
    let mut text = format!("{method} {path} HTTP/1.1\r\n");
    if !host_overridden {
        text.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("Connection: close\r\n\r\n");
    stream.write_all(text.as_bytes()).expect("send request");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no status in {status_line:?}"));
    let headers = lines
        .filter_map(|line| {
            let (k, v) = line.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect();
    Response {
        status,
        headers,
        body: body.to_string(),
    }
}

fn get(port: u16, path: &str) -> Response {
    request(port, "GET", path, &[])
}

/// Poll a read until it stops answering 503, checking each 503 says it is sampling.
fn until_sampled(port: u16, path: &str, within: Duration) -> Response {
    let started = Instant::now();
    loop {
        let r = get(port, path);
        if r.status != 503 {
            return r;
        }
        assert_sampling_reply(&r);
        assert!(
            started.elapsed() < within,
            "{path} still answers 503 after {within:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn assert_sampling_reply(r: &Response) {
    assert_eq!(r.status, 503);
    assert!(
        r.header("retry-after").is_some(),
        "a 503 must say when to retry"
    );
    let body = r.json();
    assert_eq!(body["state"], "sampling", "{}", r.body);
    assert!(body["segments"].is_array(), "{}", r.body);
}

/// Run the binary to completion, bounded, and return its exit code and stderr.
/// Used for the refusals: a server that starts when it should not would hang here,
/// so the wait is bounded and the child killed if it does not exit.
fn run_refused(args: &[&str]) -> (Option<i32>, String, String) {
    let scratch = Scratch::new();
    let stderr_path = scratch.path("stderr.txt");
    let stdout_path = scratch.path("stdout.txt");
    let mut child = ironmon()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
        .spawn()
        .expect("spawn ironmon");
    let started = Instant::now();
    let status: ExitStatus = loop {
        if let Some(status) = child.try_wait().expect("poll child") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`ironmon {}` did not refuse to start", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    (
        status.code(),
        fs::read_to_string(&stdout_path).unwrap_or_default(),
        fs::read_to_string(&stderr_path).unwrap_or_default(),
    )
}

/// The CLI's output for `args`, with its exit code.
fn cli(args: &[&str]) -> (String, Option<i32>) {
    let out = ironmon().args(args).output().expect("run ironmon");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

/// The reading object minus the sample fields the HTTP surface adds.
fn without_sample_fields(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(map) = value.as_object_mut() {
        map.remove("sampled_at");
        map.remove("age_ms");
    }
    value
}

// ---------------------------------------------------------------------------------
// The announce line and the routes
// ---------------------------------------------------------------------------------

#[test]
fn the_announce_line_is_the_first_thing_on_stdout() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    assert!(
        server.announced.starts_with("{\"listening\":\"127.0.0.1:"),
        "{:?}",
        server.announced
    );
    assert!(server.announced.ends_with("\"}\n"));
    // The banner moved to stderr and the port in the line is the one answering.
    let health = get(server.port, "/api/v1/health");
    assert_eq!(health.status, 200);
}

#[test]
fn describe_is_the_cli_document_byte_for_byte() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let r = get(server.port, "/api/v1/describe");
    assert_eq!(r.status, 200);
    let (cli_out, code) = cli(&["describe", "--format", "json"]);
    assert_eq!(code, Some(0));
    assert_eq!(r.body, cli_out, "describe must be the CLI's own output");
}

#[test]
fn get_agrees_with_the_cli_for_values_unavailable_unknown_and_templates() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let port = server.port;

    // Each case: the id as the CLI takes it, the id as it appears in the path.
    let cases = [
        ("memory.total", "memory.total"),
        // A diagnostic id: `<none>` arrives percent-encoded.
        ("disk.<none>", "disk.%3Cnone%3E"),
        ("no.such.id", "no.such.id"),
        // A template is a schema construct, not an answer.
        ("gpu.{n}.name", "gpu.%7Bn%7D.name"),
    ];
    for (cli_id, path_id) in cases {
        let (_, code) = cli(&["get", cli_id, "--format", "json"]);
        let r = until_sampled(
            port,
            &format!("/api/v1/get/{path_id}"),
            Duration::from_secs(600),
        );
        match code {
            Some(0) => {
                assert_eq!(r.status, 200, "{cli_id}: {}", r.body);
                let body = r.json();
                assert!(body.get("value").is_some(), "{cli_id} has a value: {body}");
                assert_ne!(body["provenance"], "unavailable", "{body}");
                assert!(body["sampled_at"].is_string(), "{body}");
                assert!(body["age_ms"].is_u64(), "{body}");
            }
            Some(2) => {
                assert_eq!(
                    r.status, 200,
                    "an unavailable reading is an answer: {}",
                    r.body
                );
                let body = r.json();
                assert_eq!(body["provenance"], "unavailable", "{body}");
                assert!(body.get("value").is_none(), "no value field: {body}");
                assert!(
                    body["note"].is_string(),
                    "unavailable carries a reason: {body}"
                );
            }
            Some(1) => {
                assert_eq!(r.status, 404, "{cli_id}: {}", r.body);
                assert_eq!(r.json()["error"], "no such entity id");
            }
            other => panic!("unexpected CLI exit for {cli_id}: {other:?}"),
        }
    }
}

#[test]
fn get_reading_matches_the_cli_reading_apart_from_sample_fields() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let r = until_sampled(
        server.port,
        "/api/v1/get/disk.%3Cnone%3E",
        Duration::from_secs(600),
    );
    let (cli_out, code) = cli(&["get", "disk.<none>", "--format", "json"]);
    if code == Some(2) {
        let http = without_sample_fields(r.json());
        let local: serde_json::Value = serde_json::from_str(&cli_out).unwrap();
        assert_eq!(http, local, "same reading object as the CLI");
    }
}

#[test]
fn a_cold_snapshot_answers_fast_with_503_and_later_200() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);

    let started = Instant::now();
    let first = get(server.port, "/api/v1/snapshot?domain=cpu");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the first request must not wait for a full snapshot"
    );
    match first.status {
        200 => {}
        503 => assert_sampling_reply(&first),
        other => panic!("unexpected first status {other}: {}", first.body),
    }

    let later = until_sampled(
        server.port,
        "/api/v1/snapshot?domain=cpu",
        Duration::from_secs(900),
    );
    assert_eq!(later.status, 200, "{}", later.body);
    let doc = later.json();
    assert!(doc["readings"].is_array());
    let first_reading = &doc["readings"][0];
    assert!(first_reading["sampled_at"].is_string(), "{first_reading}");
    assert!(first_reading["age_ms"].is_u64(), "{first_reading}");
}

#[test]
fn snapshot_envelope_is_the_cli_envelope_with_sample_fields_added() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &["--cors-origin", "http://localhost:5173"]);
    let r = until_sampled(
        server.port,
        "/api/v1/snapshot?domain=cpu",
        Duration::from_secs(600),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let doc = r.json();
    for key in [
        "ontology_version",
        "resolved",
        "unavailable",
        "impossible",
        "readings",
    ] {
        assert!(doc.get(key).is_some(), "CLI envelope key {key} missing");
    }
    assert!(
        doc.get("sampled_at").is_none(),
        "no envelope-wide sample time"
    );
    for reading in doc["readings"].as_array().unwrap() {
        assert!(reading["id"].as_str().unwrap().starts_with("cpu."));
        assert!(reading["provenance"].is_string());
        assert!(reading["sampled_at"].is_string());
    }
}

#[test]
fn an_unknown_domain_is_400_with_the_accepted_names() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let r = get(server.port, "/api/v1/snapshot?domain=nonsense");
    assert_eq!(r.status, 400, "{}", r.body);
    let body = r.json();
    let accepted: Vec<&str> = body["accepted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        accepted.contains(&"cpu") && accepted.contains(&"agents"),
        "{accepted:?}"
    );
}

#[test]
fn every_other_method_on_these_routes_is_405() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    for path in [
        "/api/v1/describe",
        "/api/v1/snapshot",
        "/api/v1/get/memory.total",
    ] {
        for method in ["POST", "PUT", "DELETE"] {
            let r = request(server.port, method, path, &[]);
            assert_eq!(r.status, 405, "{method} {path}");
            assert_eq!(r.header("allow"), Some("GET"));
        }
    }
}

// ---------------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------------

#[test]
fn no_cors_headers_are_sent_by_default() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let r = request(
        server.port,
        "GET",
        "/api/v1/describe",
        &[("Origin", "http://localhost:5173")],
    );
    assert_eq!(r.status, 200);
    for (name, _) in &r.headers {
        assert!(!name.starts_with("access-control-"), "unexpected {name}");
    }
    let preflight = request(
        server.port,
        "OPTIONS",
        "/api/v1/describe",
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "GET"),
        ],
    );
    assert!(preflight.header("access-control-allow-origin").is_none());
}

#[test]
fn an_exact_cors_origin_is_allowed_and_nothing_else() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &["--cors-origin", "http://localhost:5173"]);
    let allowed = request(
        server.port,
        "GET",
        "/api/v1/describe",
        &[("Origin", "http://localhost:5173")],
    );
    assert_eq!(
        allowed.header("access-control-allow-origin"),
        Some("http://localhost:5173")
    );

    let other = request(
        server.port,
        "GET",
        "/api/v1/describe",
        &[("Origin", "http://localhost:9999")],
    );
    assert!(other.header("access-control-allow-origin").is_none());

    let preflight = request(
        server.port,
        "OPTIONS",
        "/api/v1/describe",
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "GET"),
        ],
    );
    assert_eq!(preflight.status, 204);
    assert_eq!(
        preflight.header("access-control-allow-origin"),
        Some("http://localhost:5173")
    );
}

#[test]
fn a_wildcard_cors_origin_is_refused_at_startup() {
    let (code, stdout, stderr) = run_refused(&["serve", "--port", "0", "--cors-origin", "*"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("refusing `*`"), "{stderr}");
    assert!(
        !stdout.contains("listening"),
        "nothing may be announced for a refused configuration"
    );
}

// ---------------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------------

/// A key unlikely to occur anywhere by chance, so finding it in output means leakage.
fn sentinel() -> String {
    format!(
        "sentinel-key-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[test]
fn with_a_key_every_route_but_health_needs_it_and_the_key_never_leaks() {
    let scratch = Scratch::new();
    let key = sentinel();
    let key_file = scratch.path("key.txt");
    fs::write(&key_file, format!("{key}\n")).unwrap();
    let server = serve(&scratch, &["--api-key-file", key_file.to_str().unwrap()]);
    let port = server.port;
    let bearer = format!("Bearer {key}");

    // No header, a wrong key, and a wrong scheme: all 401 and none echoes anything.
    let wrong = format!("Bearer {key}x");
    let anonymous = get(port, "/api/v1/describe");
    let bad = request(
        port,
        "GET",
        "/api/v1/describe",
        &[("Authorization", &wrong)],
    );
    let scheme = request(port, "GET", "/api/v1/describe", &[("Authorization", &key)]);
    for r in [&anonymous, &bad, &scheme] {
        assert_eq!(r.status, 401, "{}", r.body);
        assert_eq!(r.header("www-authenticate"), Some("Bearer"));
        assert!(!r.body.contains(&key), "401 must not echo the key");
    }

    // The same applies to the older routes and the Prometheus exporter.
    for path in ["/health", "/api/v1/metrics/prometheus", "/api/v1/context"] {
        let r = get(port, path);
        assert_eq!(r.status, 401, "{path} must require the key");
    }

    let ok = request(
        port,
        "GET",
        "/api/v1/describe",
        &[("Authorization", &bearer)],
    );
    assert_eq!(ok.status, 200);

    // Liveness without a key reveals no sampler state, and the key-bearing reply does.
    let live = get(port, "/api/v1/health");
    assert_eq!(live.status, 200);
    assert!(live.json().get("sampler").is_none(), "{}", live.body);
    let detailed = request(port, "GET", "/api/v1/health", &[("Authorization", &bearer)]);
    assert!(
        detailed.json().get("sampler").is_some(),
        "{}",
        detailed.body
    );

    // Stdout and stderr hold no trace of the key.
    let stderr = server.stderr();
    assert!(!stderr.contains(&key), "key leaked to stderr");
    assert!(!server.announced.contains(&key), "key leaked to stdout");
}

#[test]
fn an_empty_key_file_refuses_to_start() {
    let scratch = Scratch::new();
    let key_file = scratch.path("empty.txt");
    fs::write(&key_file, "   \n").unwrap();
    let (code, stdout, stderr) = run_refused(&[
        "serve",
        "--port",
        "0",
        "--api-key-file",
        key_file.to_str().unwrap(),
    ]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("is empty"), "{stderr}");
    assert!(!stdout.contains("listening"));
}

#[test]
fn an_unreadable_key_file_refuses_to_start() {
    let scratch = Scratch::new();
    let missing = scratch.path("does-not-exist.txt");
    let (code, stdout, stderr) = run_refused(&[
        "serve",
        "--port",
        "0",
        "--api-key-file",
        missing.to_str().unwrap(),
    ]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("cannot read API key file"), "{stderr}");
    assert!(!stdout.contains("listening"));
}

#[test]
fn an_empty_key_in_the_environment_refuses_to_start() {
    let (code, stdout, stderr) = {
        let scratch = Scratch::new();
        let stderr_path = scratch.path("stderr.txt");
        let mut child = ironmon()
            .args(["serve", "--port", "0"])
            .env("IRONMON_API_KEY", "  ")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
            .spawn()
            .expect("spawn");
        let started = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if started.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                panic!("empty IRONMON_API_KEY did not refuse to start");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut out = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        (
            status.code(),
            out,
            fs::read_to_string(&stderr_path).unwrap(),
        )
    };
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("IRONMON_API_KEY is set but empty"),
        "{stderr}"
    );
    assert!(!stdout.contains("listening"));
}

// ---------------------------------------------------------------------------------
// Host header and binding
// ---------------------------------------------------------------------------------

#[test]
fn a_loopback_server_rejects_a_foreign_host_header() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &[]);
    let port = server.port;

    let evil = request(
        port,
        "GET",
        "/api/v1/describe",
        &[("Host", "rebind.example")],
    );
    assert_eq!(evil.status, 403, "{}", evil.body);
    let evil_port = request(
        port,
        "GET",
        "/api/v1/describe",
        &[("Host", "rebind.example:80")],
    );
    assert_eq!(evil_port.status, 403);

    for host in [
        format!("localhost:{port}"),
        "localhost".to_string(),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
        "[::1]".to_string(),
    ] {
        let r = request(port, "GET", "/api/v1/describe", &[("Host", &host)]);
        assert_eq!(r.status, 200, "Host {host} should be allowed");
    }
}

#[test]
fn a_taken_port_is_an_error_not_a_fallback() {
    let holder = TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let taken = holder.local_addr().unwrap().port();
    let port = taken.to_string();
    let (code, stdout, stderr) = run_refused(&["serve", "--port", &port]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("cannot listen on"), "{stderr}");
    assert!(
        !stdout.contains("listening"),
        "a server that failed to bind must not announce a port"
    );
}

#[test]
fn binding_all_interfaces_without_a_key_warns_loudly() {
    let scratch = Scratch::new();
    let server = serve(&scratch, &["--bind", "0.0.0.0"]);
    assert!(server.announced.starts_with("{\"listening\":\"0.0.0.0:"));
    // The warning is printed after the announce line, on stderr, before the server
    // blocks, so poll for it rather than assuming an order.
    let started = Instant::now();
    loop {
        if server.stderr().contains("WARNING") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "no warning on stderr"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
