//! Shared local mock HTTP server for catalog tests (no live network).
//!
//! Method-aware, path-aware, and scripted: each request consumes the
//! next scripted response and the last entry repeats. Requests are
//! recorded (method, path, body, headers) for assertions.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A scripted response.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl MockResponse {
    pub fn new(status: u16, body: &str) -> Self {
        MockResponse {
            status,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// One recorded request.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl RecordedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        let needle = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == needle)
            .map(|(_, v)| v.as_str())
    }

    /// The JSON body parsed into a value (empty body → Null).
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// A scripted local HTTP/1.1 server bound to 127.0.0.1:0.
pub struct MockServer {
    addr: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    concurrent_peak: Arc<AtomicUsize>,
    _thread: std::thread::JoinHandle<()>,
}

impl MockServer {
    pub fn start(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(responses));
        let concurrent_peak = Arc::new(AtomicUsize::new(0));
        let concurrent_now = Arc::new(AtomicUsize::new(0));
        let (rq_t, rs_t, cp_t, cn_t) = (
            requests.clone(),
            responses.clone(),
            concurrent_peak.clone(),
            concurrent_now.clone(),
        );
        let thread = std::thread::spawn(move || loop {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let (rq, rs, cp, cn) = (rq_t.clone(), rs_t.clone(), cp_t.clone(), cn_t.clone());
            let cn_inner = cn.clone();
            std::thread::spawn(move || {
                let now = cn_inner.fetch_add(1, Ordering::SeqCst) + 1;
                cp.fetch_max(now, Ordering::SeqCst);
                let mut buf = [0u8; 16_384];
                let n = stream.read(&mut buf).unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                let mut lines = raw.lines();
                let request_line = lines.next().unwrap_or_default().to_string();
                let mut headers = Vec::new();
                let mut body = String::new();
                let mut content_length = 0usize;
                for line in lines {
                    if line.is_empty() {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        let key = k.trim().to_string();
                        let value = v.trim().to_string();
                        if key.eq_ignore_ascii_case("content-length") {
                            content_length = value.parse().unwrap_or(0);
                        }
                        headers.push((key, value));
                    }
                }
                if content_length > 0 {
                    // The body may arrive in a later read; loop until we
                    // have it or the peer closes.
                    let mut rest = String::new();
                    let mut have = raw
                        .len()
                        .saturating_sub(raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len()));
                    if have > 0 {
                        rest.push_str(
                            &raw[raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len())..],
                        );
                    }
                    while have < content_length {
                        let mut more = [0u8; 4096];
                        match stream.read(&mut more) {
                            Ok(0) | Err(_) => break,
                            Ok(m) => {
                                rest.push_str(&String::from_utf8_lossy(&more[..m]));
                                have += m;
                            }
                        }
                    }
                    body = rest;
                }
                let parts: Vec<&str> = request_line.split_whitespace().collect();
                let method = parts.first().unwrap_or(&"").to_string();
                let path = parts.get(1).unwrap_or(&"").to_string();
                rq.lock().unwrap().push(RecordedRequest {
                    method,
                    path,
                    body,
                    headers,
                });
                let count = rq.lock().unwrap().len();
                let responses = rs.lock().unwrap();
                let idx = count
                    .saturating_sub(1)
                    .min(responses.len().saturating_sub(1));
                let response = responses[idx].clone();
                drop(responses);
                let reason = match response.status {
                    200 => "OK",
                    201 => "Created",
                    304 => "Not Modified",
                    400 => "Bad Request",
                    401 => "Unauthorized",
                    403 => "Forbidden",
                    404 => "Not Found",
                    422 => "Unprocessable Entity",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    503 => "Service Unavailable",
                    _ => "Status",
                };
                let mut resp = format!(
                    "HTTP/1.1 {} {}\r\ncontent-length: {}\r\n",
                    response.status,
                    reason,
                    response.body.len()
                );
                for (k, v) in &response.headers {
                    resp.push_str(&format!("{k}: {v}\r\n"));
                }
                resp.push_str("connection: close\r\n\r\n");
                resp.push_str(&response.body);
                let _ = stream.write_all(resp.as_bytes());
                cn_inner.fetch_sub(1, Ordering::SeqCst);
            });
        });
        MockServer {
            addr,
            requests,
            concurrent_peak,
            _thread: thread,
        }
    }

    /// A URL against this server.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Requests whose path contains the given substring.
    pub fn requests_for(&self, path_part: &str) -> Vec<RecordedRequest> {
        self.requests()
            .into_iter()
            .filter(|r| r.path.contains(path_part))
            .collect()
    }

    pub fn concurrent_peak(&self) -> usize {
        self.concurrent_peak.load(Ordering::SeqCst)
    }
}
