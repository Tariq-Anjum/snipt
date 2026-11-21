//! mySMS engine: Rust port of the TypeScript engine in `src/engine` (the parity
//! oracle). See docs/ARCHITECTURE.md §3 and docs/PARITY-NOTES.md for the parity
//! hazards encoded here.

#![forbid(unsafe_code)]

pub mod jsre;

pub mod budget;
pub mod cleanup;
pub mod dlt;
pub mod engine;
pub mod finance;
pub mod inbox;
pub mod ml;
pub mod otp;
pub mod reminder;
pub mod thread;
pub mod translate;
mod translate_tables;

pub use engine::{Analysis, SmsEngine};
