//! Middlewares for modifying requests and responses.
//!
//! Middlewares are used to modify requests and responses in a pipeline. They
//! are used to add functionality to the request/response cycle, such as
//! session management, adding security headers, and more.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use tower_sessions::session::{Id, Record};
use tower_sessions::{MemoryStore, SessionManagerLayer, SessionStore};

use crate::db::{model, query, Database};

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

#[derive(Debug)]
struct FlareonSessionStore {
    database: Arc<Database>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[model]
struct Session {
    id: String, // TODO make it length-limited when supported
    data: Vec<u8>,
    expiry_date: DateTime<FixedOffset>,
}

#[async_trait]
impl SessionStore for FlareonSessionStore {
    async fn create(
        &self,
        session_record: &mut Record,
    ) -> tower_sessions::session_store::Result<()> {
        todo!()
    }

    async fn save(&self, session_record: &Record) -> tower_sessions::session_store::Result<()> {
        todo!()
    }

    async fn load(&self, session_id: &Id) -> tower_sessions::session_store::Result<Option<Record>> {
        let session_record = query!(Session, $id == session_id.to_string())
            .get(self.database.as_ref())
            .await
            .map_err(|err| tower_sessions::session_store::Error::Backend(err.to_string()))?;
        Ok(session_record.map(|session| Record {
            id: session_id.clone(),
            data: session.data,
            expiry_date: session.expiry_date,
        }))
    }

    async fn delete(&self, session_id: &Id) -> tower_sessions::session_store::Result<()> {
        todo!()
    }
}

// TODO: add Flareon ORM-based session store
