//! `ironmon verify-my-profile` driven through the built binary against a loopback
//! stub standing in for `GET /v1/profile`.
//!
//! These assert the parts a library test cannot: that the command sends an
//! authenticated GET to the profile path, and that its *exit code* follows the
//! response body rather than the HTTP status — "nothing is linked to you" arrives
//! as a 200 with `profile: "absent"`, and must still read as a distinct outcome
//! from a stored profile, an unaccepted token, and a revoked install.
//!
//! The endpoint is redirected to the stub with `IRONMON_TELEMETRY_ENDPOINT`, and
//! the token supplied with `IRONMON_TELEMETRY_TOKEN`, both per child process — so
//! nothing here touches the real telemetry server or a developer's stored token,
//! and the tests do not race on shared environment.
#![cfg(feature = "remote-backends")]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn ironmon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ironmon"))
}

/// A one-shot HTTP stub on a loopback port. It accepts a single connection,
/// answers with `status`/`body`, and sends the request's start line and headers
/// back through the channel so a test can assert what the client actually sent.
fn stub(status: u16, reason: &'static str, body: &'static str) -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(request);
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    (port, rx)
}

/// Run `verify-my-profile --format json` against a stub on `port`, assert the exit
/// code, and return its stdout.
fn run(port: u16, expect_code: i32) -> String {
    let out = ironmon()
        .args(["verify-my-profile", "--format", "json"])
        .env(
            "IRONMON_TELEMETRY_ENDPOINT",
            format!("http://127.0.0.1:{port}/v1/metrics"),
        )
        .env("IRONMON_TELEMETRY_TOKEN", "dummy-token")
        .output()
        .expect("run ironmon verify-my-profile");
    assert_eq!(
        out.status.code(),
        Some(expect_code),
        "wrong exit code\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn it_sends_an_authenticated_get_to_the_profile_path() {
    let body = r#"{"install":{"appName":"rodeo","registeredVersion":null,"signals":"traces","lastSeenAt":null,"quotaUsed":0,"quotaPerDay":2000},"profile":"absent","device":null,"hint":"no device row is linked to this install"}"#;
    let (port, rx) = stub(200, "OK", body);
    run(port, 3);
    let request = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the stub received a request");
    let start = request.lines().next().unwrap_or_default();
    assert!(
        start.starts_with("GET /v1/profile "),
        "expected a GET to /v1/profile, got: {start}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer dummy-token"),
        "the request carried no bearer token:\n{request}"
    );
}

#[test]
fn a_stored_profile_exits_zero_and_shows_the_digests() {
    let body = r#"{"install":{"appName":"rodeo","registeredVersion":null,"signals":"traces","lastSeenAt":"2026-11-24T15:41:08.737Z","quotaUsed":1,"quotaPerDay":2000},"profile":"stored","device":{"matchTier":1,"lastSeenAt":"2026-11-24T15:41:08.761Z","seenCount":1,"virtType":null,"keyCount":2,"keys":{"gpu.0.uuid":"b42cb7ae","smbios_uuid":"a0860bcf"}}}"#;
    let (port, _rx) = stub(200, "OK", body);
    let stdout = run(port, 0);
    // `--format json` echoes the server body, so the digests come through verbatim.
    assert!(
        stdout.contains("b42cb7ae"),
        "stored digest missing:\n{stdout}"
    );
    assert!(
        stdout.contains("stored"),
        "profile state missing:\n{stdout}"
    );
}

#[test]
fn absent_is_a_successful_read_that_exits_three() {
    // A 200 whose body says nothing is linked is NOT an error status, but it is a
    // distinct outcome: exit 3, not 0.
    let body = r#"{"install":{"appName":"rodeo","registeredVersion":null,"signals":"traces","lastSeenAt":null,"quotaUsed":0,"quotaPerDay":2000},"profile":"absent","device":null,"hint":"no device row is linked to this install"}"#;
    let (port, _rx) = stub(200, "OK", body);
    let stdout = run(port, 3);
    assert!(
        stdout.contains("absent"),
        "profile state missing:\n{stdout}"
    );
}

#[test]
fn an_unaccepted_token_exits_two() {
    let (port, _rx) = stub(401, "Unauthorized", r#"{"error":"unauthorized"}"#);
    let stdout = run(port, 2);
    assert!(stdout.contains("unauthorized"), "status missing:\n{stdout}");
}

#[test]
fn a_revoked_install_exits_five_and_names_the_time() {
    let (port, _rx) = stub(
        403,
        "Forbidden",
        r#"{"error":"revoked","revokedAt":"2026-11-01T00:00:00Z"}"#,
    );
    let stdout = run(port, 5);
    assert!(stdout.contains("revoked"), "status missing:\n{stdout}");
    assert!(
        stdout.contains("2026-11-01T00:00:00Z"),
        "revokedAt not surfaced:\n{stdout}"
    );
}

#[test]
fn an_unreachable_server_exits_four() {
    // A port with nothing listening: the request cannot complete.
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l); // free the port so the connection is refused rather than accepted
        p
    };
    run(dead, 4);
}
