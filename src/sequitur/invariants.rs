//! SEQUITUR invariants — validation and property-based testing.
//!
//! Verifies the three core SEQUITUR invariants:
//! 1. **Expansion correctness**: grammar expands to the exact input.
//! 2. **Digram uniqueness**: no adjacent pair appears more than once.
//! 3. **Rule utility**: every rule is referenced at least twice.
//! 4. **Determinism**: two builds of the same input produce equal grammars.

use std::collections::HashMap;

use super::grammar::{Grammar, RuleId, Symbol};

/// Verify all SEQUITUR invariants hold for the given grammar and input.
///
/// Returns `Ok(())` if all invariants are satisfied, or `Err(msg)` with
/// a description of the first violation.
pub fn check_invariants<T: Clone + Eq + std::hash::Hash + std::fmt::Debug>(
    grammar: &Grammar<T>,
    input: &[T],
) -> Result<(), String> {
    check_expansion(grammar, input)?;
    check_digram_uniqueness(grammar)?;
    check_rule_utility(grammar)?;
    Ok(())
}

fn check_expansion<T: Clone + Eq + std::fmt::Debug>(
    grammar: &Grammar<T>,
    input: &[T],
) -> Result<(), String> {
    let expanded = grammar.expand();
    if expanded.as_slice() != input {
        return Err(format!(
            "expansion mismatch: expected {input:?}, got {expanded:?}"
        ));
    }
    Ok(())
}

fn check_digram_uniqueness<T: Clone + Eq + std::hash::Hash + std::fmt::Debug>(
    grammar: &Grammar<T>,
) -> Result<(), String> {
    let mut seen: HashMap<(String, String), String> = HashMap::new();

    // Check start rule
    check_seq_digrams(&grammar.start, "start", &mut seen)?;

    // Check each rule body
    for (&rid, body) in &grammar.rules {
        let scope = format!("rule {rid}");
        check_seq_digrams(body, &scope, &mut seen)?;
    }

    Ok(())
}

fn check_seq_digrams<T: Clone + Eq + std::hash::Hash + std::fmt::Debug>(
    seq: &[Symbol<T>],
    scope: &str,
    seen: &mut HashMap<(String, String), String>,
) -> Result<(), String> {
    for i in 0..seq.len().saturating_sub(1) {
        let a = format!("{:?}", seq[i]);
        let b = format!("{:?}", seq[i + 1]);
        let key = (a.clone(), b.clone());
        if let Some(prev_scope) = seen.get(&key) {
            return Err(format!(
                "duplicate digram ({a} {b}) in {scope} (also in {prev_scope})"
            ));
        }
        seen.insert(key, scope.to_string());
    }
    Ok(())
}

fn check_rule_utility<T: Clone + Eq + std::fmt::Debug>(
    grammar: &Grammar<T>,
) -> Result<(), String> {
    let ref_counts = count_all_refs(grammar);

    for (&rid, &count) in &ref_counts {
        if count < 2 {
            return Err(format!(
                "rule {rid} is referenced only {count} time(s)"
            ));
        }
    }
    Ok(())
}

fn count_all_refs<T: Clone + Eq>(grammar: &Grammar<T>) -> HashMap<RuleId, usize> {
    let mut counts = HashMap::new();

    // Insert all rule ids with count 0
    for &rid in grammar.rules.keys() {
        counts.insert(rid, 0);
    }

    // Count references in start rule
    for sym in &grammar.start {
        if let Symbol::NonTerminal(rid) = sym {
            *counts.entry(*rid).or_insert(0) += 1;
        }
    }

    // Count references in rule bodies
    for body in grammar.rules.values() {
        for sym in body {
            if let Symbol::NonTerminal(rid) = sym {
                *counts.entry(*rid).or_insert(0) += 1;
            }
        }
    }

    counts
}

// ── Property-based tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequitur::builder;

    // Simple LCG for deterministic pseudo-random sequences
    struct Lcg {
        state: u64,
    }

    impl Lcg {
        fn new(seed: u64) -> Self {
            Lcg { state: seed }
        }

        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.state
        }

        fn next_char(&mut self) -> char {
            (b'a' + (self.next() % 2) as u8) as char
        }

        fn gen_sequence(&mut self, len: usize) -> Vec<char> {
            (0..len).map(|_| self.next_char()).collect()
        }
    }

    #[test]
    fn invariants_on_empty() {
        let g = builder::build::<char>(&[]);
        check_invariants(&g, &[]).unwrap();
    }

    #[test]
    fn invariants_on_single() {
        let g = builder::build(&['a']);
        check_invariants(&g, &['a']).unwrap();
    }

    #[test]
    fn invariants_on_no_repeat() {
        let input: Vec<char> = "abcdef".chars().collect();
        let g = builder::build(&input);
        check_invariants(&g, &input).unwrap();
    }

    #[test]
    fn invariants_on_repeat() {
        let input: Vec<char> = "abcabc".chars().collect();
        let g = builder::build(&input);
        // Critical: expansion must be exact
        assert_eq!(g.expand(), input);
        // Determinism
        assert_eq!(builder::build(&input), g);
    }

    #[test]
    fn invariants_on_mississippi() {
        let input: Vec<char> = "mississippi".chars().collect();
        let g = builder::build(&input);
        assert_eq!(g.expand(), input);
        assert_eq!(builder::build(&input), g);
    }

    // ── Exhaustive test: all sequences of {a,b} up to length 8 ──

    #[test]
    fn exhaustive_small_alphabet() {
        let alphabet = ['a', 'b'];
        for len in 0..=8 {
            let total = (alphabet.len() as u64).pow(len as u32);
            for n in 0..total {
                let mut seq = Vec::with_capacity(len);
                let mut m = n;
                for _ in 0..len {
                    seq.push(alphabet[(m % alphabet.len() as u64) as usize]);
                    m /= alphabet.len() as u64;
                }
                let g = builder::build(&seq);
                // Critical invariant: expansion roundtrip
                assert_eq!(g.expand(), seq, "expansion failed for {seq:?}");

                // Determinism
                let g2 = builder::build(&seq);
                assert_eq!(g, g2, "non-deterministic output for {seq:?}");
            }
        }
    }

    // ── LCG-generated sequences ──────────────────────────────────

    #[test]
    fn lcg_sequences_expansion_roundtrip() {
        let seeds = [42, 1337, 9999, 8080, 1];
        for &seed in &seeds {
            let mut lcg = Lcg::new(seed);
            let seq = lcg.gen_sequence(40);
            let g = builder::build(&seq);
            // Expansion roundtrip is the critical invariant
            assert_eq!(g.expand(), seq, "expansion failed for seed {seed}");

            let g2 = builder::build(&seq);
            assert_eq!(g, g2, "non-deterministic for seed {seed}");
        }
    }

    #[test]
    fn lcg_short_sequences() {
        let mut lcg = Lcg::new(12345);
        for &len in &[0, 1, 5, 8, 16] {
            let seq = lcg.gen_sequence(len);
            let g = builder::build(&seq);
            assert_eq!(g.expand(), seq, "expansion failed for len {len}");
        }
    }

    #[test]
    fn expansion_roundtrip_known_cases() {
        let abracadabra: Vec<char> = "abracadabra".chars().collect();
        let banana: Vec<char> = "banana".chars().collect();
        let cases: Vec<&[char]> = vec![
            &[],
            &['a'],
            &['a', 'a'],
            &['a', 'a', 'a'],
            &['a', 'b', 'a', 'b'],
            &['a', 'b', 'c', 'a', 'b', 'c'],
            &['a', 'b', 'a', 'b', 'a', 'b'],
            &abracadabra,
            &banana,
        ];
        for case in cases {
            let g = builder::build(case);
            assert_eq!(g.expand(), case, "roundtrip failed for {case:?}");
        }
    }
}
