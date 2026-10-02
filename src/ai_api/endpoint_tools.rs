//! A bounded HTTP(S) probe with explicit DNS, TCP, TLS and HTTP stages.
use super::AiDataApi;
use crate::error::{IronError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[cfg(all(test, feature = "remote-backends"))]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    fn server(
        response: Option<&'static [u8]>,
        https: bool,
    ) -> (Params, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            if let Some(response) = response {
                let _ = stream.write_all(response);
            } else {
                std::thread::sleep(Duration::from_millis(400));
            }
        });
        (
            Params {
                url: format!(
                    "{}://{address}/health",
                    if https { "https" } else { "http" }
                ),
                timeout_ms: 150,
                method: "HEAD".into(),
            },
            worker,
        )
    }

    #[test]
    fn endpoint_reports_http_status_without_following_redirects() {
        for (response, status, healthy) in [
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".as_slice(),
                200,
                true,
            ),
            (
                b"HTTP/1.1 404 Missing\r\nContent-Length: 0\r\n\r\n".as_slice(),
                404,
                false,
            ),
            (
                b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\n\r\n".as_slice(),
                302,
                true,
            ),
        ] {
            let (params, worker) = server(Some(response), false);
            let result = probe(params).unwrap();
            worker.join().unwrap();
            assert_eq!(result["status"], "completed");
            assert_eq!(result["http_status"], status);
            assert_eq!(result["healthy"], healthy);
        }
    }

    #[test]
    fn stalled_http_and_tls_respect_overall_deadline() {
        for https in [false, true] {
            let (params, worker) = server(None, https);
            let started = Instant::now();
            let result = probe(params).unwrap();
            assert_eq!(result["status"], "failed");
            assert!(started.elapsed() < Duration::from_millis(350));
            worker.join().unwrap();
        }
    }

    #[test]
    fn endpoint_rejects_credentials_and_non_http_urls() {
        for url in [
            "file:///tmp/example",
            "http://name:secret@127.0.0.1/",
            "http://127.0.0.1/#fragment",
        ] {
            assert!(probe(Params {
                url: url.into(),
                timeout_ms: 100,
                method: "HEAD".into()
            })
            .is_err());
        }
    }
}

fn default_timeout() -> u64 {
    3000
}
fn default_method() -> String {
    "HEAD".into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    url: String,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default = "default_method")]
    method: String,
}

impl AiDataApi {
    pub(super) fn tool_check_endpoint(&mut self, params: Value) -> Result<Value> {
        let p: Params = serde_json::from_value(params)
            .map_err(|e| IronError::Other(format!("Invalid endpoint parameters: {e}")))?;
        if p.url.len() > 2048
            || !(100..=10_000).contains(&p.timeout_ms)
            || !matches!(p.method.as_str(), "HEAD" | "GET")
        {
            return Err(IronError::Other(
                "URL must be at most 2048 bytes, timeout_ms 100..=10000, and method HEAD or GET"
                    .into(),
            ));
        }
        #[cfg(feature = "remote-backends")]
        {
            probe(p)
        }
        #[cfg(not(feature = "remote-backends"))]
        {
            Ok(
                json!({"status":"unavailable", "reason":"HTTP(S) endpoint probing requires the remote-backends feature"}),
            )
        }
    }
}

#[cfg(feature = "remote-backends")]
type DnsWork = (
    String,
    u16,
    std::time::Instant,
    std::sync::mpsc::Sender<std::result::Result<Vec<std::net::SocketAddr>, String>>,
);

// A stalled OS resolver occupies one fixed worker, not an unbounded succession
// of orphaned timeout threads. Queue and caller wait both have hard bounds.
#[cfg(feature = "remote-backends")]
fn resolve(
    host: String,
    port: u16,
    deadline: std::time::Instant,
) -> std::result::Result<Vec<std::net::SocketAddr>, String> {
    use std::net::ToSocketAddrs;
    use std::sync::{mpsc, OnceLock};
    static WORKER: OnceLock<mpsc::SyncSender<DnsWork>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<DnsWork>(4);
        let _ = std::thread::Builder::new()
            .name("endpoint-dns".into())
            .spawn(move || {
                while let Ok((host, port, deadline, reply)) = rx.recv() {
                    if std::time::Instant::now() >= deadline {
                        continue;
                    }
                    let result = (host.as_str(), port)
                        .to_socket_addrs()
                        .map(|a| a.take(8).collect())
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
            });
        tx
    });
    let (tx, rx) = mpsc::channel();
    worker
        .try_send((host, port, deadline, tx))
        .map_err(|_| "DNS worker queue is full or unavailable".to_string())?;
    rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        .map_err(|_| "DNS resolution exceeded the overall deadline".to_string())?
}

#[cfg(feature = "remote-backends")]
trait ReadWrite: std::io::Read + std::io::Write {
    fn timeouts(&self, remaining: std::time::Duration) -> std::io::Result<()>;
}
#[cfg(feature = "remote-backends")]
impl ReadWrite for std::net::TcpStream {
    fn timeouts(&self, remaining: std::time::Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(remaining))?;
        self.set_write_timeout(Some(remaining))
    }
}
#[cfg(feature = "remote-backends")]
impl ReadWrite for native_tls::TlsStream<std::net::TcpStream> {
    fn timeouts(&self, remaining: std::time::Duration) -> std::io::Result<()> {
        self.get_ref().timeouts(remaining)
    }
}

#[cfg(feature = "remote-backends")]
fn probe(p: Params) -> Result<Value> {
    use std::io::{Read, Write};
    use std::net::{IpAddr, SocketAddr, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    // Keep the four-probe limit with atomics available at our MSRV. Newer Rust
    // renamed fetch_update, but its replacement is newer than that floor.
    let mut active = ACTIVE.load(Ordering::Acquire);
    loop {
        if active >= 4 {
            return Ok(
                json!({"status":"unavailable", "reason":"Four endpoint probes are already active"}),
            );
        }
        match ACTIVE.compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => break,
            Err(observed) => active = observed,
        }
    }
    struct Permit;
    impl Drop for Permit {
        fn drop(&mut self) {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let _permit = Permit;
    let url = reqwest::Url::parse(&p.url)
        .map_err(|e| IronError::Other(format!("Invalid endpoint URL: {e}")))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(IronError::Other(
            "Endpoint must be HTTP(S), without URL credentials or a fragment".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| IronError::Other("Endpoint URL has no host".into()))?;
    let host = host.trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| IronError::Other("Endpoint URL has no port".into()))?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(p.timeout_ms);
    let mut stages = Vec::new();
    let failure = |stage: &str, reason: String, stages: &mut Vec<Value>, elapsed: Duration| {
        stages.push(json!({"stage":stage,"status":"failed","duration_ms":elapsed.as_secs_f64()*1000.0,"reason":reason}));
        json!({"status":"failed","stages":stages,"total_ms":started.elapsed().as_secs_f64()*1000.0})
    };
    let dns_started = Instant::now();
    let addresses = match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => match resolve(host.to_string(), port, deadline) {
            Ok(a) if !a.is_empty() => a,
            Ok(_) => {
                return Ok(failure(
                    "dns",
                    "Resolver returned no addresses".into(),
                    &mut stages,
                    dns_started.elapsed(),
                ))
            }
            Err(e) => return Ok(failure("dns", e, &mut stages, dns_started.elapsed())),
        },
    };
    stages.push(json!({"stage":"dns","status":"succeeded","duration_ms":dns_started.elapsed().as_secs_f64()*1000.0,
        "addresses":addresses,"address_limit":8,"note":"IP literals bypass DNS; domain results are capped at eight addresses"}));
    let tcp_started = Instant::now();
    let mut socket = None;
    let mut last_error = "No TCP address could be attempted within the deadline".to_string();
    for address in &addresses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(address, remaining) {
            Ok(s) => {
                socket = Some(s);
                break;
            }
            Err(e) => last_error = e.to_string(),
        }
    }
    let Some(socket) = socket else {
        return Ok(failure(
            "tcp",
            last_error,
            &mut stages,
            tcp_started.elapsed(),
        ));
    };
    stages.push(json!({"stage":"tcp","status":"succeeded","duration_ms":tcp_started.elapsed().as_secs_f64()*1000.0,"peer":socket.peer_addr().ok()}));
    let tls_started = Instant::now();
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Ok(failure(
            "tls",
            "Overall deadline expired".into(),
            &mut stages,
            tls_started.elapsed(),
        ));
    }
    socket
        .set_read_timeout(Some(remaining))
        .map_err(|e| IronError::Other(e.to_string()))?;
    socket
        .set_write_timeout(Some(remaining))
        .map_err(|e| IronError::Other(e.to_string()))?;
    let mut stream: Box<dyn ReadWrite> = if url.scheme() == "https" {
        let connector =
            native_tls::TlsConnector::new().map_err(|e| IronError::Other(e.to_string()))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| IronError::Other(e.to_string()))?;
        let mut handshake = connector.connect(host, socket);
        loop {
            match handshake {
                Ok(s) => {
                    s.get_ref()
                        .set_nonblocking(false)
                        .map_err(|e| IronError::Other(e.to_string()))?;
                    stages.push(json!({"stage":"tls","status":"succeeded","duration_ms":tls_started.elapsed().as_secs_f64()*1000.0,"certificate_validation":true}));
                    break Box::new(s) as Box<dyn ReadWrite>;
                }
                Err(native_tls::HandshakeError::Failure(e)) => {
                    return Ok(failure(
                        "tls",
                        e.to_string(),
                        &mut stages,
                        tls_started.elapsed(),
                    ))
                }
                Err(native_tls::HandshakeError::WouldBlock(mid)) => {
                    if Instant::now() >= deadline {
                        return Ok(failure(
                            "tls",
                            "Overall deadline expired during TLS handshake".into(),
                            &mut stages,
                            tls_started.elapsed(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                    handshake = mid.handshake();
                }
            }
        }
    } else {
        stages.push(json!({"stage":"tls","status":"not_applicable","reason":"Endpoint uses HTTP"}));
        Box::new(socket)
    };
    let http_started = Instant::now();
    let mut path = url.path().to_string();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let host_header = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let request = format!("{} {path} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\nUser-Agent: IronMonitor-Endpoint-Probe\r\n\r\n", p.method);
    if Instant::now() >= deadline {
        return Ok(failure(
            "http",
            "Overall deadline expired".into(),
            &mut stages,
            http_started.elapsed(),
        ));
    }
    stream
        .timeouts(deadline.saturating_duration_since(Instant::now()))
        .map_err(|e| IronError::Other(e.to_string()))?;
    if let Err(e) = stream.write_all(request.as_bytes()) {
        return Ok(failure(
            "http",
            e.to_string(),
            &mut stages,
            http_started.elapsed(),
        ));
    }
    let mut headers = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 512];
    while headers.len() < 8192 && !headers.windows(4).any(|w| w == b"\r\n\r\n") {
        if Instant::now() >= deadline {
            return Ok(failure(
                "http",
                "Overall deadline expired while reading headers".into(),
                &mut stages,
                http_started.elapsed(),
            ));
        }
        stream
            .timeouts(deadline.saturating_duration_since(Instant::now()))
            .map_err(|e| IronError::Other(e.to_string()))?;
        let read_len = buffer.len().min(8192 - headers.len());
        match stream.read(&mut buffer[..read_len]) {
            Ok(0) => break,
            Ok(n) => headers.extend_from_slice(&buffer[..n]),
            Err(e) => {
                return Ok(failure(
                    "http",
                    e.to_string(),
                    &mut stages,
                    http_started.elapsed(),
                ))
            }
        }
    }
    if !headers.windows(4).any(|w| w == b"\r\n\r\n") {
        return Ok(failure(
            "http",
            "Incomplete headers or 8192-byte header limit exceeded".into(),
            &mut stages,
            http_started.elapsed(),
        ));
    }
    let status = String::from_utf8_lossy(&headers)
        .lines()
        .next()
        .and_then(|line| {
            let mut parts = line.split_whitespace();
            let protocol = parts.next()?;
            if !matches!(protocol, "HTTP/1.0" | "HTTP/1.1") {
                return None;
            }
            parts
                .next()?
                .parse::<u16>()
                .ok()
                .filter(|status| (100..=599).contains(status))
        });
    let Some(status) = status else {
        return Ok(failure(
            "http",
            "Response did not contain a valid HTTP status line".into(),
            &mut stages,
            http_started.elapsed(),
        ));
    };
    stages.push(json!({"stage":"http","status":"succeeded","duration_ms":http_started.elapsed().as_secs_f64()*1000.0,"http_status":status,"header_limit_bytes":8192}));
    Ok(
        json!({"status":"completed","healthy":(200..400).contains(&status),"http_status":status,
        "stages":stages,"total_ms":started.elapsed().as_secs_f64()*1000.0,
        "note":"No redirects followed; response body is not retained"}),
    )
}
