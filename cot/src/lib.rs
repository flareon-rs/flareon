//! Cot is an easy to use, modern, and fast web framework for Rust. It has
//! been designed to be familiar if you've ever used
//! [Django](https://www.djangoproject.com/), and easy to learn if you haven't.
//! It's a batteries-included framework built on top of
//! [axum](https://github.com/tokio-rs/axum).
//!
//! ## Features
//!
//! * **Easy to use API** — in many ways modeled after Django, Cot's API is
//!   designed to be easy to use and intuitive. Sensible defaults make it for
//!   easy rapid development, while the API is still empowering you when needed.
//!   The documentation is a first-class citizen in Cot, making it easy to find
//!   what you're looking for.
//! * **ORM integration** — Cot comes with its own ORM, allowing you to interact
//!   with your database in a way that feels Rusty and intuitive. Rust types are
//!   the source of truth, and the ORM takes care of translating them to and
//!   from the database, as well as creating the migrations automatically.
//! * **Type safe** — wherever possible, Cot uses Rust's type system to prevent
//!   common mistakes and bugs. Not only views are taking advantage of the
//!   Rust's type system, but also the ORM, the admin panel, and even the
//!   templates. All that to catch errors as early as possible.
//! * **Admin panel** — Cot comes with an admin panel out of the box, allowing
//!   you to manage your app's data with ease. Adding new models to the admin
//!   panel is stupidly simple, making it a great tool not only for rapid
//!   development and debugging, but with its customization options, also for
//!   production use.
//! * **Secure by default** — security should be opt-out, not opt-in. Cot takes
//!   care of making your web apps secure by default, defending it against
//!   common modern web vulnerabilities. You can focus on building your app, not
//!   securing it.
//!
//! ## Guide
//!
//! This is an API reference for Cot, which might not be the best place to
//! start learning Cot. For a more gentle introduction, see the
//! [Cot guide](https://cot.rs/guide/latest/).
//!
//! ## Examples
//!
//! To see examples of how to use Cot, see the
//! [examples in the repository](https://github.com/cot-rs/cot/tree/master/examples).

#![warn(missing_docs, rustdoc::missing_crate_level_docs)]
#![cfg_attr(
    docsrs,
    feature(doc_cfg, doc_auto_cfg, rustdoc_missing_doc_code_examples)
)]
#![cfg_attr(docsrs, warn(rustdoc::missing_doc_code_examples))]

extern crate self as cot;

#[cfg(feature = "db")]
pub mod db;
mod error;
pub mod form;
mod headers;
// Not public API. Referenced by macro-generated code.
#[doc(hidden)]
#[path = "private.rs"]
pub mod __private;
pub mod admin;
pub mod auth;
mod body;
pub mod cli;
pub mod config;
mod error_page;
mod handler;
pub mod html;
pub mod middleware;
pub(crate) mod project;
pub mod request;
pub mod response;
pub mod router;
pub mod static_files;
pub mod test;
pub(crate) mod utils;

pub use body::Body;
pub use cot_macros::main;
pub use error::Error;
pub use {bytes, http};

pub use crate::handler::{BoxedHandler, RequestHandler};
pub use crate::project::{run, run_at, run_cli, AppContext, CotApp, CotProject, CotProjectBuilder};

/// A type alias for a result that can return a [`cot::Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// A type alias for an HTTP status code.
pub type StatusCode = http::StatusCode;

/// A type alias for an HTTP method.
pub type Method = http::Method;
