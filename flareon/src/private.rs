//! Re-exports of some of the Flareon dependencies that are used in the macros.
//!
//! This is to avoid the need to add them as dependencies to the crate that uses
//! the macros.
//!
//! This is not a public API and should not be used directly.

pub use async_trait::async_trait;
pub use bytes::Bytes;
pub use tokio;
