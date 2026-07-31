//! inim — Internetwork Impact Monitor
//!
//! A minimal, rigorous Rust application that determines how planned or
//! unplanned network events affect the globally visible routing system.

pub mod domain;
pub mod sources;
pub mod ingest;
pub mod routes;
pub mod tokenize;
pub mod sequitur;
pub mod waves;
pub mod assess;
pub mod report;
pub mod manifest;
pub mod discover;
pub mod target;
pub mod output;
pub mod outcome;
pub mod orchestrate;
pub mod fixtures;
