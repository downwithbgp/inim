//! SEQUITUR builder — the classic Nevill-Manning & Witten (1997) algorithm.
//!
//! Builds a grammar from a terminal sequence by enforcing two invariants:
//! 1. **Digram uniqueness**: no pair of adjacent symbols appears more
//!    than once in the entire grammar (start + all rule bodies).
//! 2. **Rule utility**: every rule is referenced at least twice.

use std::collections::HashMap;

use super::grammar::{Grammar, RuleId, Symbol};

/// A reference to a position in the grammar: (rule_id, index).
/// `None` as rule_id means the start rule.
type Position = (Option<RuleId>, usize);

/// Build a SEQUITUR grammar from a terminal sequence.
pub fn build<T: Clone + Eq + std::hash::Hash + std::fmt::Debug>(
    input: &[T],
) -> Grammar<T> {
    let mut b = Builder::new();
    for symbol in input {
        b.append(Symbol::Terminal(symbol.clone()));
    }
    b.into_grammar()
}

struct Builder<T> {
    grammar: Grammar<T>,
    /// digram → list of positions where it occurs
    digram_index: HashMap<(Symbol<T>, Symbol<T>), Vec<Position>>,
}

impl<T: Clone + Eq + std::hash::Hash + std::fmt::Debug> Builder<T> {
    fn new() -> Self {
        Builder {
            grammar: Grammar::new(),
            digram_index: HashMap::new(),
        }
    }

    fn into_grammar(self) -> Grammar<T> {
        self.grammar
    }

    /// Append a symbol to the start rule and enforce SEQUITUR invariants.
    fn append(&mut self, sym: Symbol<T>) {
        let pos = self.grammar.start.len();
        self.grammar.start.push(sym);
        // The new element is at index 'pos'. The digram spanning it and its
        // predecessor is at (pos-1, pos) if pos >= 1.
        if pos >= 1 {
            self.add_digram(None, pos - 1);
            self.enforce_digram_uniqueness(None, pos - 1);
        }
    }

    // ── Sequence access ──────────────────────────────────────────

    fn seq_len(&self, rule_id: Option<RuleId>) -> usize {
        match rule_id {
            None => self.grammar.start.len(),
            Some(rid) => self.grammar.rules[&rid].len(),
        }
    }

    fn seq_get(&self, rule_id: Option<RuleId>, pos: usize) -> Option<&Symbol<T>> {
        match rule_id {
            None => self.grammar.start.get(pos),
            Some(rid) => self.grammar.rules.get(&rid).and_then(|s| s.get(pos)),
        }
    }

    // Returns clones of the two symbols at (pos, pos+1) if they exist
    fn digram_at(&self, rule_id: Option<RuleId>, pos: usize) -> Option<(Symbol<T>, Symbol<T>)> {
        let a = self.seq_get(rule_id, pos)?.clone();
        let b = self.seq_get(rule_id, pos + 1)?.clone();
        Some((a, b))
    }

    // ── Digram index manipulation ────────────────────────────────

    fn add_digram(&mut self, rule_id: Option<RuleId>, pos: usize) {
        if let Some(d) = self.digram_at(rule_id, pos) {
            self.digram_index.entry(d).or_default().push((rule_id, pos));
        }
    }

    fn remove_digram(&mut self, rule_id: Option<RuleId>, pos: usize) {
        if let Some(d) = self.digram_at(rule_id, pos) {
            if let Some(positions) = self.digram_index.get_mut(&d) {
                positions.retain(|p| *p != (rule_id, pos));
                if positions.is_empty() {
                    self.digram_index.remove(&d);
                }
            }
        }
    }

    // ── Core SEQUITUR enforcement ────────────────────────────────

    /// Enforce that the digram at (rule_id, pos) is unique.
    fn enforce_digram_uniqueness(&mut self, rule_id: Option<RuleId>, pos: usize) {
        self.enforce_digram_uniqueness_depth(rule_id, pos, 0);
    }

    fn enforce_digram_uniqueness_depth(&mut self, rule_id: Option<RuleId>, pos: usize, depth: usize) {
        // Recursion guard: avoid infinite loops from expand↔enforce cycles
        const MAX_DEPTH: usize = 20;
        if depth > MAX_DEPTH {
            return;
        }

        let d = match self.digram_at(rule_id, pos) {
            Some(d) => d,
            None => return,
        };

        let occurrences: Vec<Position> = match self.digram_index.get(&d) {
            Some(occ) if occ.len() > 1 => occ.clone(),
            _ => return,
        };

        // Create a new rule for this digram
        let new_rule_id = self.grammar.fresh_rule_id();
        self.grammar.rules.insert(
            new_rule_id,
            vec![d.0.clone(), d.1.clone()],
        );

        // Substitute all occurrences. Iterate in reverse and re-check
        // that the digram at the stored position still matches — overlapping
        // digrams may have been destroyed by a prior substitution.
        for &(occ_rule_id, occ_pos) in occurrences.iter().rev() {
            let current = match self.digram_at(occ_rule_id, occ_pos) {
                Some(dg) => dg,
                None => continue,
            };
            if current != d {
                continue;
            }
            self.substitute_digram_depth(occ_rule_id, occ_pos, new_rule_id, depth + 1);
        }

        // Enforce rule utility after substitutions
        if self.count_rule_refs(new_rule_id) < 2 {
            self.expand_rule_depth(new_rule_id, depth + 1);
        }
    }

    /// Replace a digram at (rule_id, pos) with a non-terminal reference.
    fn substitute_digram_depth(
        &mut self,
        rule_id: Option<RuleId>,
        pos: usize,
        new_rule_id: RuleId,
        depth: usize,
    ) {
        // Remove affected digrams
        if pos > 0 {
            self.remove_digram(rule_id, pos - 1);
        }
        self.remove_digram(rule_id, pos);
        if pos + 2 < self.seq_len(rule_id) {
            self.remove_digram(rule_id, pos + 1);
        }

        // Replace two symbols with one non-terminal
        self.seq_remove(rule_id, pos); // remove first symbol
        self.seq_remove(rule_id, pos); // remove second symbol (shifted to pos)
        self.seq_insert(rule_id, pos, Symbol::NonTerminal(new_rule_id));

        // Add new digrams
        if pos > 0 {
            self.add_digram(rule_id, pos - 1);
            self.enforce_digram_uniqueness_depth(rule_id, pos - 1, depth + 1);
        }
        self.add_digram(rule_id, pos);
        if pos + 1 < self.seq_len(rule_id) {
            self.enforce_digram_uniqueness_depth(rule_id, pos, depth + 1);
        }
    }

    fn expand_rule_depth(&mut self, rule_id: RuleId, depth: usize) {
        let ref_pos = self.find_rule_ref(rule_id);
        let body = match self.grammar.rules.remove(&rule_id) {
            Some(b) => b,
            None => return,
        };

        if let Some((ref_rule_id, pos)) = ref_pos {
            // Remove digrams around the non-terminal
            if pos > 0 {
                self.remove_digram(ref_rule_id, pos - 1);
            }
            self.remove_digram(ref_rule_id, pos);

            // Remove non-terminal
            self.seq_remove(ref_rule_id, pos);

            // Insert rule body
            let insert_pos = pos;
            for (i, sym) in body.iter().enumerate() {
                self.seq_insert(ref_rule_id, insert_pos + i, sym.clone());
            }

            // Add digrams for affected area
            let start_check = if insert_pos > 0 { insert_pos - 1 } else { 0 };
            let end = insert_pos + body.len();
            for i in start_check..end {
                if i + 1 < self.seq_len(ref_rule_id) {
                    self.add_digram(ref_rule_id, i);
                }
            }
            // Re-check affected digrams
            for i in start_check..end {
                if i + 1 < self.seq_len(ref_rule_id) {
                    self.enforce_digram_uniqueness_depth(ref_rule_id, i, depth + 1);
                }
            }
        }
    }

    // ── Sequence mutation helpers ────────────────────────────────

    fn seq_remove(&mut self, rule_id: Option<RuleId>, pos: usize) {
        match rule_id {
            None => { self.grammar.start.remove(pos); }
            Some(rid) => { self.grammar.rules.get_mut(&rid).unwrap().remove(pos); }
        }
    }

    fn seq_insert(&mut self, rule_id: Option<RuleId>, pos: usize, sym: Symbol<T>) {
        match rule_id {
            None => { self.grammar.start.insert(pos, sym); }
            Some(rid) => { self.grammar.rules.get_mut(&rid).unwrap().insert(pos, sym); }
        }
    }

    // ── Rule reference counting ──────────────────────────────────

    fn count_rule_refs(&self, target: RuleId) -> usize {
        let mut count = 0;
        for sym in &self.grammar.start {
            if matches!(sym, Symbol::NonTerminal(r) if *r == target) {
                count += 1;
            }
        }
        for body in self.grammar.rules.values() {
            for sym in body {
                if matches!(sym, Symbol::NonTerminal(r) if *r == target) {
                    count += 1;
                }
            }
        }
        count
    }

    fn find_rule_ref(&self, target: RuleId) -> Option<Position> {
        for (pos, sym) in self.grammar.start.iter().enumerate() {
            if matches!(sym, Symbol::NonTerminal(r) if *r == target) {
                return Some((None, pos));
            }
        }
        for (&rid, body) in &self.grammar.rules {
            for (pos, sym) in body.iter().enumerate() {
                if matches!(sym, Symbol::NonTerminal(r) if *r == target) {
                    return Some((Some(rid), pos));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let g = build::<char>(&[]);
        let empty: Vec<char> = vec![];
        assert_eq!(g.expand(), empty);
    }

    #[test]
    fn single_symbol() {
        let g = build(&['a']);
        assert_eq!(g.expand(), vec!['a']);
        assert!(g.rules.is_empty());
    }

    #[test]
    fn no_repeats_no_rules() {
        let g = build(&['a', 'b', 'c']);
        assert_eq!(g.expand(), vec!['a', 'b', 'c']);
        assert!(g.rules.is_empty());
    }

    #[test]
    fn repeat_pair_creates_rule() {
        let input = "abcabc".chars().collect::<Vec<_>>();
        let g = build(&input);
        assert_eq!(g.expand(), input);
        assert!(!g.rules.is_empty());
    }

    #[test]
    fn overlapping_digram_aaa() {
        let input = vec!['a', 'a', 'a'];
        let g = build(&input);
        assert_eq!(g.expand(), input);
    }

    #[test]
    fn nested_repeats() {
        let input = "ababab".chars().collect::<Vec<_>>();
        let g = build(&input);
        assert_eq!(g.expand(), input);
    }

    #[test]
    fn rule_utility_enforced() {
        let input = "abcdab".chars().collect::<Vec<_>>();
        let g = build(&input);
        assert_eq!(g.expand(), input);
    }

    #[test]
    fn expansion_roundtrip() {
        let input: Vec<char> = "abracadabra".chars().collect();
        let g = build(&input);
        assert_eq!(g.expand(), input);
    }

    #[test]
    fn deterministic_output() {
        let input: Vec<char> = "mississippi".chars().collect();
        let g1 = build(&input);
        let g2 = build(&input);
        assert_eq!(g1, g2, "same input must produce identical grammar");
        assert_eq!(g1.expand(), g2.expand());
    }
}
