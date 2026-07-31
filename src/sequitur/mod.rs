//! SEQUITUR module — grammar inference for route-transition sequences.
//!
//! TODO: Implement the SEQUITUR algorithm for discovering repeated and
//! hierarchical structure in token sequences. This module must have no
//! BGP-specific knowledge.

/// A SEQUITUR grammar that can compress and analyze symbol sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grammar;

impl Grammar {
    /// Create a new empty grammar.
    pub fn new() -> Self {
        Grammar
    }
}

impl Default for Grammar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_grammar_exists() {
        let g = Grammar::new();
        assert_eq!(g, Grammar::default());
    }
}
