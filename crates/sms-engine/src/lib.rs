//! mySMS engine: Rust port of the TypeScript engine in `src/engine` (the parity
//! oracle). See docs/ARCHITECTURE.md §3 for the parity hazards encoded here.

#![forbid(unsafe_code)]

pub mod jsre;

pub mod dlt;
pub mod engine;
pub mod finance;
pub mod ml;
pub mod otp;
pub mod reminder;
pub mod thread;

pub use engine::{Analysis, SmsEngine};
