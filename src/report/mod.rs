//! Report module — deterministic human-readable and JSON output.
//!
//! TODO: Implement terminal and JSON report rendering from assessments.

use serde_json::Value;

/// Render a terminal (human-readable) report.
pub fn render_terminal() -> String {
    // TODO: implement
    "inim: report not yet implemented".to_string()
}

/// Render a structured JSON report.
pub fn render_json() -> Value {
    // TODO: implement
    Value::String("inim: report not yet implemented".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_render_terminal_returns_string() {
        let report = render_terminal();
        assert!(!report.is_empty());
    }

    #[test]
    fn stub_render_json_returns_value() {
        let json = render_json();
        assert!(json.is_string());
    }
}
