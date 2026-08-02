//! Web server — localhost-only, read-only.
//!
//! HTTP requests never perform Broker discovery, downloads, MRT parsing,
//! or analysis. The default bind is loopback; non-loopback binds require
//! an explicit flag and print a warning that there is no authentication.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::Router;

use super::AppState;

/// Parse and validate the bind address.
///
/// Default policy: loopback only. Non-loopback requires an explicit
/// `allow_non_loopback` flag (the initial application has no auth).
pub fn validate_bind(bind: &str, allow_non_loopback: bool) -> Result<SocketAddr, String> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| format!("invalid bind address {bind:?}: {e}"))?;
    let is_loopback = match addr.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    };
    if !is_loopback && !allow_non_loopback {
        return Err(format!(
            "bind address {bind} is not loopback. The initial application has no authentication; \
             pass --allow-non-loopback to bind explicitly."
        ));
    }
    Ok(addr)
}

/// Build the application state for a catalog database.
pub fn build_state(
    db_path: &Path,
    catalog_root: &Path,
    software_version: &str,
    writes_enabled: bool,
) -> Result<Arc<AppState>, String> {
    let conn = if db_path.exists() {
        super::super::db::open_catalog(db_path)?
    } else {
        return Err(format!(
            "catalog database {} does not exist; run 'inim catalog init --db {}' first",
            db_path.display(),
            db_path.display()
        ));
    };
    Ok(Arc::new(AppState {
        db: Mutex::new(conn),
        catalog_root: catalog_root.to_path_buf(),
        software_version: software_version.to_string(),
        writes_enabled,
        csrf_token: super::generate_csrf_token(),
    }))
}

/// Build the application router (used by the CLI and by tests).
pub fn build_app(state: Arc<AppState>) -> Router {
    super::build_router(state)
}

/// Run the web server with graceful shutdown on Ctrl-C / SIGTERM.
pub async fn serve(
    db_path: &Path,
    catalog_root: &Path,
    bind: &str,
    allow_non_loopback: bool,
    enable_writes: bool,
    allow_unauthenticated_writes: bool,
    software_version: &str,
) -> Result<(), String> {
    let addr = validate_bind(bind, allow_non_loopback)?;
    if !addr.ip().is_loopback() {
        if enable_writes && !allow_unauthenticated_writes {
            return Err(
                "refusing to enable writes on a non-loopback bind: write mode is unauthenticated \
                 and intended for trusted local use only; pass --allow-unauthenticated-writes to \
                 acknowledge this explicitly"
                    .to_string(),
            );
        }
        eprintln!("WARNING: binding to {addr} (non-loopback). There is no authentication.");
    }
    let state = build_state(db_path, catalog_root, software_version, enable_writes)?;
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot bind {addr}: {e}"))?;
    if enable_writes {
        eprintln!(
            "inim catalog web UI listening on http://{addr} (WRITE MODE ENABLED: unauthenticated \
             local mutations with CSRF; no analysis on request path)"
        );
    } else {
        eprintln!(
            "inim catalog web UI listening on http://{addr} (read-only, no analysis on request path)"
        );
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| format!("server error: {e}"))
}

async fn shutdown_signal() {
    // SIGINT and SIGTERM both stop accepting requests and finish
    // in-flight short mutations; the web process never owns worker
    // execution, so no job state is touched here.
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("cannot install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bind_is_loopback() {
        assert!(validate_bind("127.0.0.1:8080", false).is_ok());
        assert!(validate_bind("[::1]:8080", false).is_ok());
    }

    #[test]
    fn non_loopback_bind_requires_explicit_value() {
        assert!(validate_bind("0.0.0.0:8080", false).is_err());
        assert!(validate_bind("192.168.1.5:8080", false).is_err());
        // Explicit opt-in works but the CLI prints the warning.
        assert!(validate_bind("0.0.0.0:8080", true).is_ok());
    }

    #[test]
    fn invalid_bind_is_rejected() {
        assert!(validate_bind("not-an-address", false).is_err());
        assert!(validate_bind("127.0.0.1", false).is_err());
    }
}
