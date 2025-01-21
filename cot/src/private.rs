//! Re-exports of some of the Cot dependencies that are used in the macros
//! and the CLI.
//!
//! This is to avoid the need to add them as dependencies to the crate that uses
//! the macros.
//!
//! This is not a public API and should not be used directly.

pub use async_trait::async_trait;
pub use bytes::Bytes;
/// Rinja's macros don't work when Rinja is re-exported, so there's no point in
/// re-exporting it publicly. However, we need to re-export it here so that our
/// macros can implement traits from Rinja.
pub use rinja;
pub use tokio;

// used in the CLI
#[cfg(feature = "db")]
pub use crate::utils::graph::apply_permutation;
