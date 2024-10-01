//! Test utilities for Flareon projects.

use std::future::poll_fn;
use std::sync::Arc;

use derive_more::Debug;
use flareon::{prepare_request, FlareonProject};
use tower::Service;
use tower_sessions::{MemoryStore, Session};

use crate::config::ProjectConfig;
use crate::db::migrations::{DynMigration, DynMigrationWrapper, MigrationEngine};
use crate::db::Database;
use crate::request::Request;
use crate::response::Response;
use crate::{Body, Error, Result};

/// A test client for making requests to a Flareon project.
///
/// Useful for End-to-End testing Flareon projects.
#[derive(Debug)]
pub struct Client<S> {
    project: FlareonProject<S>,
}

impl<S> Client<S>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    #[must_use]
    pub fn new(project: FlareonProject<S>) -> Self {
        Self { project }
    }

    pub async fn get(&mut self, path: &str) -> Result<Response> {
        self.request(
            http::Request::get(path)
                .body(Body::empty())
                .expect("Test request should be valid"),
        )
        .await
    }

    pub async fn request(&mut self, mut request: Request) -> Result<Response> {
        prepare_request(
            &mut request,
            self.project.config.clone(),
            self.project.router.clone(),
        );

        poll_fn(|cx| self.project.handler.poll_ready(cx)).await?;
        self.project.handler.call(request).await
    }
}

#[derive(Debug, Clone)]
pub struct TestRequestBuilder {
    method: http::Method,
    url: String,
    has_session: bool,
    config: Option<Arc<ProjectConfig>>,
}

impl TestRequestBuilder {
    #[must_use]
    pub fn get(url: &str) -> Self {
        Self {
            method: http::Method::GET,
            url: url.to_string(),
            has_session: false,
            config: None,
        }
    }

    #[must_use]
    pub fn with_session(&mut self) -> &mut Self {
        self.has_session = true;
        self
    }

    #[must_use]
    pub fn with_config(&mut self, config: ProjectConfig) -> &mut Self {
        self.config = Some(Arc::new(config));
        self
    }

    #[must_use]
    pub fn with_default_config(&mut self) -> &mut Self {
        self.config = Some(Arc::new(ProjectConfig::default()));
        self
    }

    #[must_use]
    pub fn build(&mut self) -> http::Request<Body> {
        let mut request = http::Request::builder()
            .method(self.method.clone())
            .uri(self.url.clone())
            .body(Body::empty())
            .expect("Test request should be valid");

        if self.has_session {
            let session_store = MemoryStore::default();
            let session = Session::new(None, Arc::new(session_store), None);
            request.extensions_mut().insert(session);
        }

        if let Some(config) = &self.config {
            request.extensions_mut().insert(config.clone());
        }

        request
    }
}

#[derive(Debug)]
struct TestDatabaseBuilder {
    migrations: Vec<DynMigrationWrapper>,
}

impl TestDatabaseBuilder {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    pub(crate) fn add_migrations<T: DynMigration + 'static, V: IntoIterator<Item = T>>(
        mut self,
        migrations: V,
    ) -> Self {
        self.migrations
            .extend(migrations.into_iter().map(DynMigrationWrapper::new));
        self
    }

    async fn build(self) -> Database {
        let engine = MigrationEngine::new(self.migrations);
        let database = Database::new("sqlite::memory:").await.unwrap();
        engine.run(&database).await.unwrap();
        database
    }
}
