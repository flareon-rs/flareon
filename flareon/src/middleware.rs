//! Middlewares for modifying requests and responses.
//!
//! Middlewares are used to modify requests and responses in a pipeline. They
//! are used to add functionality to the request/response cycle, such as
//! session management, adding security headers, and more.

use tower_sessions::{MemoryStore, SessionManagerLayer};

#[derive(Debug, Copy, Clone)]
pub struct SessionMiddleware;

impl SessionMiddleware {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SessionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> tower::Layer<S> for SessionMiddleware {
    type Service = <SessionManagerLayer<MemoryStore> as tower::Layer<S>>::Service;

    fn layer(&self, inner: S) -> Self::Service {
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store);
        session_layer.layer(inner)
    }
}

// TODO: add Flareon ORM-based session store
