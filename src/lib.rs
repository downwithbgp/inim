//! inim — Internetwork Impact Monitor
//!
//! A minimal, rigorous Rust application that determines how planned or
//! unplanned network events affect the globally visible routing system.

pub mod assess;
pub mod catalog;
pub mod cohort;
pub mod compare;
pub mod conventions;
pub mod derived_cache;
pub mod discover;
pub mod domain;
pub mod execution;
pub mod fixtures;
pub mod ingest;
pub mod lifecycle;
pub mod manifest;
pub mod observability;
pub mod orchestrate;
pub mod outcome;
pub mod output;
pub mod perf;
pub mod pipeline;
pub mod plan;
pub mod profiles;
pub mod report;
pub mod routes;
pub mod schema;
pub mod sequitur;
pub mod sources;
pub mod target;
pub mod tokenize;
pub mod waves;
pub mod worker;
