//! Tokenize module — conversion from route-state changes to canonical
//! transition symbols.
//!
//! TODO: Implement transition symbol emission based on route-state diffs.

/// A canonical transition symbol (e.g. "ANNOUNCEMENT", "WITHDRAWAL").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransitionSymbol(String);

/// Convert a sequence of route-state changes into canonical transition symbols.
pub fn emit_transitions() -> Vec<TransitionSymbol> {
    // TODO: implement
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_emit_transitions_returns_empty() {
        let symbols = emit_transitions();
        assert!(symbols.is_empty());
    }
}
