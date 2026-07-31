//! SEQUITUR module — grammar inference for symbol sequences.
//!
//! Implements the classic Nevill-Manning & Witten (1997) algorithm
//! for discovering repeated and hierarchical structure in token
//! sequences. This module has **no** BGP, MRT, RouteViews, or
//! Internet2 knowledge — it operates on abstract symbol sequences.
//!
//! ## Submodules
//! - `grammar`: data structures (`Grammar<T>`, `Symbol<T>`, `RuleId`),
//!   expansion, and rendering.
//! - `builder`: the SEQUITUR algorithm (`build()`), enforcing digram
//!   uniqueness and rule utility.
//! - `invariants`: validation (`check_invariants()`) and property-based
//!   tests (exhaustive small-alphabet + LCG-generated sequences).

pub mod builder;
pub mod grammar;
pub mod invariants;

pub use builder::build;
pub use grammar::{Grammar, RuleId, Symbol};
