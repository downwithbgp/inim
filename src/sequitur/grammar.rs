//! SEQUITUR grammar — data structures, expansion, and rendering.
//!
//! This module has no BGP, MRT, RouteViews, or Internet2 knowledge.
//! It operates on abstract symbol sequences.

use std::collections::HashMap;
use std::fmt;

/// A rule identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(pub u32);

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R{}", self.0)
    }
}

/// A symbol in the grammar: either a terminal (input symbol) or a
/// non-terminal (reference to a rule).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol<T> {
    Terminal(T),
    NonTerminal(RuleId),
}

impl<T: fmt::Display> fmt::Display for Symbol<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Symbol::Terminal(t) => write!(f, "{t}"),
            Symbol::NonTerminal(r) => write!(f, "{r}"),
        }
    }
}

/// A SEQUITUR grammar.
///
/// `start` is the top-level symbol sequence. `rules` maps rule ids to
/// their expansion sequences. The grammar is always cycle-free and
/// digram-unique by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grammar<T> {
    pub start: Vec<Symbol<T>>,
    pub rules: HashMap<RuleId, Vec<Symbol<T>>>,
    next_rule_id: RuleId,
}

impl<T: Clone> Grammar<T> {
    pub fn new() -> Self {
        Grammar {
            start: Vec::new(),
            rules: HashMap::new(),
            next_rule_id: RuleId(0),
        }
    }

    /// Allocate a fresh rule id.
    pub fn fresh_rule_id(&mut self) -> RuleId {
        let id = self.next_rule_id;
        self.next_rule_id = RuleId(id.0 + 1);
        id
    }
}

impl<T: Clone> Default for Grammar<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Eq> Grammar<T> {
    /// Expand the grammar to recover the original terminal sequence.
    pub fn expand(&self) -> Vec<T> {
        let mut result = Vec::new();
        self.expand_seq(&self.start, &mut result, &mut Vec::new());
        result
    }

    fn expand_seq(&self, seq: &[Symbol<T>], out: &mut Vec<T>, stack: &mut Vec<RuleId>) {
        for sym in seq {
            match sym {
                Symbol::Terminal(t) => out.push(t.clone()),
                Symbol::NonTerminal(rid) => {
                    // Cycle detection (should never happen in valid grammar)
                    if stack.contains(rid) {
                        continue;
                    }
                    if let Some(body) = self.rules.get(rid) {
                        stack.push(*rid);
                        self.expand_seq(body, out, stack);
                        stack.pop();
                    }
                }
            }
        }
    }
}

impl<T: Clone + Eq + std::fmt::Display> Grammar<T> {
    /// Render the grammar's root structure as a compact string.
    ///
    /// Shows the start rule with non-terminals expanded one level for
    /// readability. Used as the wave motif.
    pub fn render_root(&self) -> String {
        let mut parts = Vec::new();
        for sym in &self.start {
            match sym {
                Symbol::Terminal(t) => {
                    parts.push(format!("{t}"));
                }
                Symbol::NonTerminal(rid) => {
                    if let Some(body) = self.rules.get(rid) {
                        let inner: Vec<String> = body.iter().map(|s| format!("{s}")).collect();
                        parts.push(format!("[{}]", inner.join(" ")));
                    } else {
                        parts.push(format!("{rid}"));
                    }
                }
            }
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_grammar_expands_to_empty() {
        let g: Grammar<char> = Grammar::new();
        let empty: Vec<char> = vec![];
        assert_eq!(g.expand(), empty);
    }

    #[test]
    fn terminal_only_grammar() {
        let mut g = Grammar::new();
        g.start = vec![Symbol::Terminal('a'), Symbol::Terminal('b')];
        assert_eq!(g.expand(), vec!['a', 'b']);
    }

    #[test]
    fn single_rule_expansion() {
        let mut g = Grammar::new();
        let r0 = g.fresh_rule_id();
        g.rules
            .insert(r0, vec![Symbol::Terminal('a'), Symbol::Terminal('b')]);
        g.start = vec![Symbol::NonTerminal(r0), Symbol::Terminal('c')];
        assert_eq!(g.expand(), vec!['a', 'b', 'c']);
    }

    #[test]
    fn nested_rule_expansion() {
        let mut g = Grammar::new();
        let r0 = g.fresh_rule_id();
        let r1 = g.fresh_rule_id();
        g.rules
            .insert(r0, vec![Symbol::Terminal('b'), Symbol::Terminal('c')]);
        g.rules
            .insert(r1, vec![Symbol::Terminal('a'), Symbol::NonTerminal(r0)]);
        g.start = vec![Symbol::NonTerminal(r1), Symbol::Terminal('d')];
        assert_eq!(g.expand(), vec!['a', 'b', 'c', 'd']);
    }

    #[test]
    fn render_root_simple() {
        let mut g = Grammar::new();
        g.start = vec![Symbol::Terminal('a'), Symbol::Terminal('b')];
        assert_eq!(g.render_root(), "a b");
    }

    #[test]
    fn render_root_with_rule() {
        let mut g = Grammar::new();
        let r0 = g.fresh_rule_id();
        g.rules
            .insert(r0, vec![Symbol::Terminal('x'), Symbol::Terminal('y')]);
        g.start = vec![Symbol::NonTerminal(r0), Symbol::Terminal('z')];
        assert_eq!(g.render_root(), "[x y] z");
    }
}
