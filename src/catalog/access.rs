//! Conservative HTTP access policy for public source acquisition
//! (Session 33, Part 2).
//!
//! Until an official policy or explicit permission provides different
//! limits, public source acquisition uses a self-imposed conservative
//! default:
//!
//! - maximum concurrent requests: **1**
//! - sustained request rate: **0.25 requests/second** (one request every
//!   four seconds)
//! - burst: **1**
//! - default request budget per sync: **100**
//!
//! The client honors conditional requests (If-None-Match /
//! If-Modified-Since), `Retry-After`, exponential backoff with bounded
//! jitter, and documented stop conditions. A higher rate requires an
//! explicit flag at the CLI (never a default).
//!
//! State/priority label maps elsewhere are lossless label translations;
//! this module contains no severity or priority computation.

use std::time::{Duration, Instant};

/// Default maximum concurrent requests (strictly serial).
pub const DEFAULT_MAX_CONCURRENCY: usize = 1;
/// Default sustained request rate (one request every four seconds).
pub const DEFAULT_REQUESTS_PER_SECOND: f64 = 0.25;
/// Default burst size.
pub const DEFAULT_BURST: usize = 1;
/// Default request budget per sync.
pub const DEFAULT_MAX_REQUESTS: usize = 100;
/// Transient-failure retries per request (429/503/5xx).
pub const DEFAULT_MAX_RETRIES: u32 = 3;
/// Backoff base delay for the first retry.
pub const DEFAULT_BACKOFF_BASE_MS: u64 = 500;
/// Backoff ceiling.
pub const DEFAULT_BACKOFF_MAX_MS: u64 = 30_000;
/// Maximum jitter added on top of the exponential backoff (0.5 = ±50%).
pub const JITTER_FRACTION: f64 = 0.5;

/// Self-imposed acquisition limits, all configurable but conservative by
/// default. Raising them requires an explicit call (CLI flags).
#[derive(Debug, Clone)]
pub struct AccessPolicy {
    pub max_concurrency: usize,
    pub requests_per_second: f64,
    pub burst: usize,
    pub max_requests: usize,
    /// Transient-failure retries per request.
    pub max_retries: u32,
    /// Base backoff delay (ms) for the first retry.
    pub backoff_base_ms: u64,
    /// Backoff ceiling (ms).
    pub backoff_max_ms: u64,
    /// Optional contact for the User-Agent. Never invented.
    pub contact: Option<String>,
}

impl Default for AccessPolicy {
    fn default() -> Self {
        AccessPolicy::conservative()
    }
}

impl AccessPolicy {
    /// The self-imposed conservative default.
    pub fn conservative() -> Self {
        AccessPolicy {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            requests_per_second: DEFAULT_REQUESTS_PER_SECOND,
            burst: DEFAULT_BURST,
            max_requests: DEFAULT_MAX_REQUESTS,
            max_retries: DEFAULT_MAX_RETRIES,
            backoff_base_ms: DEFAULT_BACKOFF_BASE_MS,
            backoff_max_ms: DEFAULT_BACKOFF_MAX_MS,
            contact: None,
        }
    }

    /// Validate the policy; a nonsensical configuration is rejected.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_concurrency == 0 {
            return Err("max_concurrency must be >= 1".to_string());
        }
        if self.requests_per_second <= 0.0 || !self.requests_per_second.is_finite() {
            return Err("requests_per_second must be positive and finite".to_string());
        }
        if self.burst == 0 {
            return Err("burst must be >= 1".to_string());
        }
        if self.max_requests == 0 {
            return Err(
                "max_requests must be >= 1 (an unbounded crawl is not allowed)".to_string(),
            );
        }
        if self.backoff_base_ms == 0 {
            return Err("backoff_base_ms must be >= 1".to_string());
        }
        Ok(())
    }

    /// Minimum interval between requests (seconds).
    pub fn min_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.requests_per_second)
    }

    /// Descriptive User-Agent: inim version + project purpose + optional
    /// contact. Never invents a contact address.
    pub fn user_agent(&self) -> String {
        let version = env!("CARGO_PKG_VERSION");
        match &self.contact {
            Some(c) if !c.is_empty() => {
                format!("inim/{version} (public-ticket corpus research; contact {c})")
            }
            _ => format!("inim/{version} (public-ticket corpus research; no contact configured)"),
        }
    }
}

/// Outcome of one polite fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// 200 with a body.
    Ok(FetchedBody),
    /// 304 Not Modified — the caller's stored content is still current.
    NotModified,
    /// 404 — permanent; never retried.
    NotFound,
    /// 403 — terminal stop condition.
    Forbidden,
    /// 401 — unexpected authentication requirement; terminal stop.
    Unauthorized,
}

/// A successfully fetched body plus the response validators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedBody {
    pub body: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Why the client stopped before the caller's work was complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The per-sync request budget is exhausted.
    BudgetExhausted,
    /// A 403 was received (repeated 403s also land here).
    Forbidden,
    /// The source unexpectedly requires authentication.
    AuthenticationRequired,
    /// Too many consecutive 429 responses.
    RepeatedRateLimited,
}

/// Client error: terminal condition or transport failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    Stop(StopReason),
    /// Transient failures exhausted their retries.
    TooManyRetries(String),
    /// A status that is neither retryable nor a known terminal outcome.
    UnexpectedStatus(u16),
    Transport(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Stop(s) => write!(f, "sync stopped: {s:?}"),
            ClientError::TooManyRetries(detail) => write!(f, "retries exhausted: {detail}"),
            ClientError::UnexpectedStatus(s) => write!(f, "unexpected HTTP status {s}"),
            ClientError::Transport(detail) => write!(f, "transport error: {detail}"),
        }
    }
}

// ── Tiny deterministic RNG (no new dependency) ─────────────────────

/// xorshift64* — small deterministic PRNG for bounded jitter.
pub struct JitterRng(u64);

impl JitterRng {
    pub fn new(seed: u64) -> Self {
        JitterRng(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform float in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Exponential backoff with bounded jitter: `base * 2^attempt` scaled by
/// `(1 + jitter * u)` with `u` uniform in [0,1), capped at the ceiling.
pub fn backoff_delay(attempt: u32, base_ms: u64, max_ms: u64, rng: &mut JitterRng) -> Duration {
    let exp = base_ms.saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(max_ms);
    let jittered = capped as f64 * (1.0 + JITTER_FRACTION * rng.next_f64());
    Duration::from_millis(jittered as u64)
}

/// Bounds of `backoff_delay` for a given attempt (without RNG).
pub fn backoff_bounds(attempt: u32, base_ms: u64, max_ms: u64) -> (Duration, Duration) {
    let exp = base_ms.saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(max_ms);
    (
        Duration::from_millis(capped),
        Duration::from_millis((capped as f64 * (1.0 + JITTER_FRACTION)) as u64),
    )
}

// ── Polite client ──────────────────────────────────────────────────

/// A strictly serial, rate-limited, budget-bounded HTTP client for
/// public source acquisition. One request at a time by construction
/// (concurrency never exceeds the policy because requests are issued
/// sequentially from the sync loop).
pub struct PoliteClient {
    policy: AccessPolicy,
    http: reqwest::blocking::Client,
    rng: JitterRng,
    /// Requests made in total (including retries).
    requests_made: u64,
    /// Remaining per-sync budget.
    budget_remaining: usize,
    last_request_at: Option<Instant>,
}

impl PoliteClient {
    pub fn new(policy: AccessPolicy) -> Result<Self, String> {
        PoliteClient::new_with_seed(policy, 0x5EED_2026)
    }

    /// `new` with an explicit RNG seed (deterministic tests).
    pub fn new_with_seed(policy: AccessPolicy, seed: u64) -> Result<Self, String> {
        policy.validate()?;
        let http = reqwest::blocking::Client::builder()
            .user_agent(policy.user_agent())
            .build()
            .map_err(|e| format!("cannot build HTTP client: {e}"))?;
        Ok(PoliteClient {
            policy,
            http,
            rng: JitterRng::new(seed),
            requests_made: 0,
            budget_remaining: usize::MAX,
            last_request_at: None,
        })
    }

    pub fn policy(&self) -> &AccessPolicy {
        &self.policy
    }

    pub fn requests_made(&self) -> u64 {
        self.requests_made
    }

    pub fn budget_remaining(&self) -> usize {
        self.budget_remaining
    }

    /// Set the per-sync request budget for this client instance.
    pub fn set_budget(&mut self, budget: usize) {
        self.budget_remaining = budget;
    }

    /// Enforce the sustained request rate (sleep until the next slot).
    fn pace(&mut self) {
        let interval = self.policy.min_interval();
        if let Some(last) = self.last_request_at {
            let elapsed = last.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
    }

    /// One HTTP request against the source. Assumes the caller checked
    /// the budget; consumes one budget unit and one rate slot.
    fn send(
        &mut self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<reqwest::blocking::Response, ClientError> {
        self.pace();
        self.last_request_at = Some(Instant::now());
        self.requests_made += 1;
        self.budget_remaining = self.budget_remaining.saturating_sub(1);
        let mut req = self.http.get(url);
        if let Some(e) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, e);
        }
        if let Some(im) = last_modified {
            req = req.header(reqwest::header::IF_MODIFIED_SINCE, im);
        }
        req.send()
            .map_err(|e| ClientError::Transport(e.to_string()))
    }

    /// Parse a `Retry-After` header (seconds or HTTP-date). Returns None
    /// when absent or unparseable — the caller falls back to backoff.
    fn retry_after(response: &reqwest::blocking::Response) -> Option<Duration> {
        let value = response.headers().get(reqwest::header::RETRY_AFTER)?;
        let text = value.to_str().ok()?.trim();
        if let Ok(secs) = text.parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
        // HTTP-date form: parse as RFC 2822-ish (chrono).
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(text) {
            let now = chrono::Utc::now();
            let target = dt.with_timezone(&chrono::Utc);
            if target > now {
                return Some((target - now).to_std().unwrap_or(Duration::from_secs(1)));
            }
        }
        None
    }

    fn body_of(response: reqwest::blocking::Response) -> Result<FetchedBody, ClientError> {
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let body = response
            .text()
            .map_err(|e| ClientError::Transport(format!("cannot read response body: {e}")))?;
        Ok(FetchedBody {
            body,
            status,
            content_type,
            etag,
            last_modified,
        })
    }

    /// Fetch one URL with retries, conditional validators, and stop
    /// conditions.
    ///
    /// - 304 → `NotModified` (no body, no new content).
    /// - 404 → `NotFound`, never retried.
    /// - 403 → `Forbidden` (stop). 401 → `Unauthorized` (stop).
    /// - 429/503/5xx → retried with `Retry-After` or bounded backoff up
    ///   to `max_retries`; repeated 429s stop the sync.
    /// - Budget exhaustion → `Stop(BudgetExhausted)`.
    pub fn fetch(
        &mut self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<FetchOutcome, ClientError> {
        let mut attempt: u32 = 0;
        loop {
            if self.budget_remaining == 0 {
                return Err(ClientError::Stop(StopReason::BudgetExhausted));
            }
            let response = self.send(url, etag, last_modified)?;
            let status = response.status().as_u16();
            match status {
                200 => return Ok(FetchOutcome::Ok(Self::body_of(response)?)),
                304 => return Ok(FetchOutcome::NotModified),
                404 => return Ok(FetchOutcome::NotFound),
                401 => return Err(ClientError::Stop(StopReason::AuthenticationRequired)),
                403 => return Err(ClientError::Stop(StopReason::Forbidden)),
                429 => {
                    let retry_after = Self::retry_after(&response);
                    drop(response);
                    if attempt >= self.policy.max_retries {
                        return Err(ClientError::Stop(StopReason::RepeatedRateLimited));
                    }
                    match retry_after {
                        Some(d) => std::thread::sleep(d),
                        None => std::thread::sleep(backoff_delay(
                            attempt,
                            self.policy.backoff_base_ms,
                            self.policy.backoff_max_ms,
                            &mut self.rng,
                        )),
                    }
                    attempt += 1;
                }
                503 | 500 | 502 | 504 => {
                    let retry_after = Self::retry_after(&response);
                    drop(response);
                    if attempt >= self.policy.max_retries {
                        return Err(ClientError::TooManyRetries(format!(
                            "status {status} after {} retries",
                            self.policy.max_retries
                        )));
                    }
                    match retry_after {
                        Some(d) => std::thread::sleep(d),
                        None => std::thread::sleep(backoff_delay(
                            attempt,
                            self.policy.backoff_base_ms,
                            self.policy.backoff_max_ms,
                            &mut self.rng,
                        )),
                    }
                    attempt += 1;
                }
                other => return Err(ClientError::UnexpectedStatus(other)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rate_is_conservative() {
        let p = AccessPolicy::default();
        assert_eq!(p.max_concurrency, 1);
        assert_eq!(p.burst, 1);
        assert_eq!(p.max_requests, 100);
        assert!((p.requests_per_second - 0.25).abs() < 1e-9);
        // One request every four seconds.
        assert_eq!(p.min_interval(), Duration::from_secs(4));
        // User-Agent names the tool, its version, and its purpose; no
        // invented contact appears when none is configured.
        let ua = p.user_agent();
        assert!(ua.starts_with("inim/"), "{ua}");
        assert!(ua.contains("public-ticket corpus research"), "{ua}");
        assert!(!ua.contains('@'), "no invented contact: {ua}");
        let with_contact = AccessPolicy {
            contact: Some("ops@example.invalid".to_string()),
            ..p.clone()
        };
        assert!(with_contact.user_agent().contains("ops@example.invalid"));
    }

    #[test]
    fn invalid_policies_are_rejected() {
        assert!(AccessPolicy {
            max_concurrency: 0,
            ..AccessPolicy::default()
        }
        .validate()
        .is_err());
        assert!(AccessPolicy {
            max_requests: 0,
            ..AccessPolicy::default()
        }
        .validate()
        .is_err());
        assert!(AccessPolicy {
            requests_per_second: 0.0,
            ..AccessPolicy::default()
        }
        .validate()
        .is_err());
        assert!(AccessPolicy::default().validate().is_ok());
    }

    #[test]
    fn jitter_is_bounded() {
        let mut rng = JitterRng::new(42);
        for attempt in 0..6u32 {
            let (lo, hi) = backoff_bounds(attempt, 500, 30_000);
            for _ in 0..200 {
                let d = backoff_delay(attempt, 500, 30_000, &mut rng);
                assert!(d >= lo, "attempt {attempt}: {d:?} < {lo:?}");
                assert!(d <= hi, "attempt {attempt}: {d:?} > {hi:?}");
            }
        }
        // The ceiling is respected regardless of attempt count.
        let (_, hi) = backoff_bounds(40, 500, 30_000);
        assert!(hi <= Duration::from_millis(45_000));
    }

    // ── Local mock HTTP server (no live network) ────────────────────

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// (status, headers, body)
    type MockResponse = (u16, Vec<(String, String)>, String);

    struct MockServer {
        addr: String,
        requests: Arc<AtomicUsize>,
        /// (status, headers, body) responses consumed in order; the last
        /// one repeats.
        _script: Arc<Mutex<Vec<MockResponse>>>,
        /// Assertion hook: receives each raw request line + headers.
        seen_headers: Arc<Mutex<Vec<String>>>,
        /// Max concurrent requests observed.
        concurrent_peak: Arc<AtomicUsize>,
    }

    impl MockServer {
        fn start(script: Vec<MockResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let requests = Arc::new(AtomicUsize::new(0));
            let _script = Arc::new(Mutex::new(script));
            let seen_headers = Arc::new(Mutex::new(Vec::new()));
            let concurrent_peak = Arc::new(AtomicUsize::new(0));
            let concurrent_now = Arc::new(AtomicUsize::new(0));
            let (rq_t, sc_t, sh_t, cp_t, cn_t) = (
                requests.clone(),
                _script.clone(),
                seen_headers.clone(),
                concurrent_peak.clone(),
                concurrent_now.clone(),
            );
            std::thread::spawn(move || loop {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let (rq, sc, sh, cp, cn) = (
                    rq_t.clone(),
                    sc_t.clone(),
                    sh_t.clone(),
                    cp_t.clone(),
                    cn_t.clone(),
                );
                let cn_inner = cn.clone();
                std::thread::spawn(move || {
                    let now = cn_inner.fetch_add(1, Ordering::SeqCst) + 1;
                    cp.fetch_max(now, Ordering::SeqCst);
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                    sh.lock().unwrap().push(raw.clone());
                    let count = rq.fetch_add(1, Ordering::SeqCst);
                    let script = sc.lock().unwrap();
                    let idx = count.min(script.len().saturating_sub(1));
                    let (status, headers, body) = script[idx].clone();
                    drop(script);
                    let reason = match status {
                        200 => "OK",
                        304 => "Not Modified",
                        403 => "Forbidden",
                        404 => "Not Found",
                        429 => "Too Many Requests",
                        503 => "Service Unavailable",
                        _ => "Status",
                    };
                    let mut resp = format!(
                        "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\n",
                        body.len()
                    );
                    for (k, v) in &headers {
                        resp.push_str(&format!("{k}: {v}\r\n"));
                    }
                    resp.push_str("connection: close\r\n\r\n");
                    resp.push_str(&body);
                    let _ = stream.write_all(resp.as_bytes());
                    cn_inner.fetch_sub(1, Ordering::SeqCst);
                });
            });
            MockServer {
                addr,
                requests,
                _script,
                seen_headers,
                concurrent_peak,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{}", self.addr, path)
        }

        fn request_count(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }

        fn has_header(&self, name: &str) -> bool {
            let seen = self.seen_headers.lock().unwrap();
            seen.iter().any(|raw| {
                raw.lines().any(|l| {
                    l.to_ascii_lowercase()
                        .starts_with(&name.to_ascii_lowercase())
                })
            })
        }

        fn concurrent_peak(&self) -> usize {
            self.concurrent_peak.load(Ordering::SeqCst)
        }

        fn policy_fast() -> AccessPolicy {
            AccessPolicy {
                requests_per_second: 1000.0,
                backoff_base_ms: 1,
                backoff_max_ms: 5,
                max_retries: 2,
                ..AccessPolicy::default()
            }
        }
    }

    #[test]
    fn concurrency_never_exceeds_configured_limit() {
        // Even with a policy that permits more concurrency, the client is
        // strictly serial: the server never observes overlapping requests.
        let server = MockServer::start(vec![(200, vec![], r#"{"ok":true}"#.to_string())]);
        let policy = AccessPolicy {
            max_concurrency: 4,
            ..MockServer::policy_fast()
        };
        let mut client = PoliteClient::new(policy).unwrap();
        for _ in 0..5 {
            let out = client.fetch(&server.url("/t"), None, None).unwrap();
            assert!(matches!(out, FetchOutcome::Ok(_)));
        }
        assert_eq!(server.request_count(), 5);
        assert_eq!(server.concurrent_peak(), 1, "requests must not overlap");
    }

    #[test]
    fn request_budget_stops_sync_cleanly() {
        let server = MockServer::start(vec![(200, vec![], "{}".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        client.set_budget(3);
        for _ in 0..3 {
            assert!(matches!(
                client.fetch(&server.url("/t"), None, None).unwrap(),
                FetchOutcome::Ok(_)
            ));
        }
        let err = client.fetch(&server.url("/t"), None, None).unwrap_err();
        assert_eq!(err, ClientError::Stop(StopReason::BudgetExhausted));
        assert_eq!(server.request_count(), 3, "budgeted requests only");
        assert_eq!(client.requests_made(), 3);
        assert_eq!(client.budget_remaining(), 0);
    }

    #[test]
    fn retry_after_is_honored() {
        let server = MockServer::start(vec![
            (
                429,
                vec![("retry-after".to_string(), "1".to_string())],
                "{}".to_string(),
            ),
            (200, vec![], "{}".to_string()),
        ]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let start = Instant::now();
        let out = client.fetch(&server.url("/t"), None, None).unwrap();
        assert!(matches!(out, FetchOutcome::Ok(_)));
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "Retry-After: 1 must be honored"
        );
        assert_eq!(server.request_count(), 2);
    }

    #[test]
    fn etag_generates_conditional_request() {
        let server = MockServer::start(vec![(304, vec![], "".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let out = client
            .fetch(
                &server.url("/t"),
                Some("\"abc123\""),
                Some("Wed, 21 Aug 2019 04:00:00 GMT"),
            )
            .unwrap();
        assert_eq!(out, FetchOutcome::NotModified);
        assert!(
            server.has_header("if-none-match"),
            "If-None-Match must be sent"
        );
        assert!(
            server.has_header("if-modified-since"),
            "If-Modified-Since must be sent"
        );
    }

    #[test]
    fn not_modified_does_not_create_snapshot() {
        // At the client level: a 304 outcome carries no body and is
        // reported as NotModified, so the caller has nothing to snapshot.
        let server = MockServer::start(vec![(304, vec![], "".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let out = client
            .fetch(&server.url("/t"), Some("\"v1\""), None)
            .unwrap();
        assert_eq!(out, FetchOutcome::NotModified);
        // A subsequent fetch with the same validator is also 304 (the
        // server script repeats the last entry).
        let out2 = client
            .fetch(&server.url("/t"), Some("\"v1\""), None)
            .unwrap();
        assert_eq!(out2, FetchOutcome::NotModified);
        assert_eq!(server.request_count(), 2);
    }

    #[test]
    fn repeated_forbidden_response_stops_sync() {
        let server = MockServer::start(vec![(403, vec![], "{}".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let err = client.fetch(&server.url("/t"), None, None).unwrap_err();
        assert_eq!(err, ClientError::Stop(StopReason::Forbidden));
        // The stop is terminal: further fetches also stop without
        // hammering the source (the caller aborts the sync).
        let err2 = client.fetch(&server.url("/t"), None, None).unwrap_err();
        assert_eq!(err2, ClientError::Stop(StopReason::Forbidden));
        assert_eq!(server.request_count(), 2);
    }

    #[test]
    fn permanent_not_found_is_not_retried_indefinitely() {
        let server = MockServer::start(vec![(404, vec![], "{}".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let out = client.fetch(&server.url("/missing"), None, None).unwrap();
        assert_eq!(out, FetchOutcome::NotFound);
        // Exactly one request: 404 is terminal, never retried.
        assert_eq!(server.request_count(), 1);
    }

    #[test]
    fn sync_can_resume_after_budget_exhaustion() {
        let server = MockServer::start(vec![(200, vec![], "{}".to_string())]);
        // First client exhausts its budget mid-sync.
        let mut first = PoliteClient::new(MockServer::policy_fast()).unwrap();
        first.set_budget(2);
        assert!(matches!(
            first.fetch(&server.url("/t"), None, None).unwrap(),
            FetchOutcome::Ok(_)
        ));
        assert!(matches!(
            first.fetch(&server.url("/t"), None, None).unwrap(),
            FetchOutcome::Ok(_)
        ));
        assert!(matches!(
            first.fetch(&server.url("/t"), None, None),
            Err(ClientError::Stop(StopReason::BudgetExhausted))
        ));
        // A new client (next sync run) continues from the persisted
        // frontier with a fresh budget.
        let mut second = PoliteClient::new(MockServer::policy_fast()).unwrap();
        second.set_budget(100);
        assert!(matches!(
            second.fetch(&server.url("/t"), None, None).unwrap(),
            FetchOutcome::Ok(_)
        ));
        assert_eq!(server.request_count(), 3);
    }

    #[test]
    fn repeated_rate_limited_response_stops_sync() {
        let server = MockServer::start(vec![(429, vec![], "{}".to_string())]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let err = client.fetch(&server.url("/t"), None, None).unwrap_err();
        assert_eq!(err, ClientError::Stop(StopReason::RepeatedRateLimited));
        // 1 initial + 2 retries with max_retries=2.
        assert_eq!(server.request_count(), 3);
    }

    #[test]
    fn transient_server_error_is_retried_then_succeeds() {
        let server = MockServer::start(vec![
            (503, vec![], "{}".to_string()),
            (503, vec![], "{}".to_string()),
            (200, vec![], "{}".to_string()),
        ]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let out = client.fetch(&server.url("/t"), None, None).unwrap();
        assert!(matches!(out, FetchOutcome::Ok(_)));
        assert_eq!(server.request_count(), 3);
    }

    #[test]
    fn body_validators_are_captured() {
        let server = MockServer::start(vec![(
            200,
            vec![
                ("etag".to_string(), "\"v9\"".to_string()),
                (
                    "last-modified".to_string(),
                    "Wed, 21 Aug 2019 04:00:00 GMT".to_string(),
                ),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            r#"{"total":1,"result":[]}"#.to_string(),
        )]);
        let mut client = PoliteClient::new(MockServer::policy_fast()).unwrap();
        let FetchOutcome::Ok(body) = client.fetch(&server.url("/t"), None, None).unwrap() else {
            panic!("expected Ok");
        };
        assert_eq!(body.body, r#"{"total":1,"result":[]}"#);
        assert_eq!(body.etag.as_deref(), Some("\"v9\""));
        assert_eq!(
            body.last_modified.as_deref(),
            Some("Wed, 21 Aug 2019 04:00:00 GMT")
        );
        assert_eq!(body.content_type.as_deref(), Some("application/json"));
    }
}
