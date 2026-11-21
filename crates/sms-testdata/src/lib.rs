//! Dev/test-only data: the 27-message demo seed and the 10,000+ history
//! generator. Not linked into release app builds except behind a DEV import.

#![forbid(unsafe_code)]

pub mod bulk;
pub mod seed;

pub use bulk::generate_bulk_history_messages;
pub use seed::create_seed_messages;

/// `MIN_HISTORY_CAPACITY` from indexedDb.ts.
pub const MIN_HISTORY_CAPACITY: usize = 10_000;
