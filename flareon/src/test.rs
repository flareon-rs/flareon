//! Test utilities for Flareon projects.

use std::future::poll_fn;
use std::sync::Arc;

use derive_more::Debug;
use flareon::{prepare_request, FlareonProject};
use tower::Service;
use tower_sessions::{MemoryStore, Session};

use crate::auth::db::DatabaseUserBackend;
use crate::config::ProjectConfig;
use crate::db::migrations::{DynMigration, MigrationEngine, MigrationWrapper};
use crate::db::Database;
use crate::request::{Request, RequestExt};
use crate::response::Response;
use crate::router::Router;
use crate::{AppContext, Body, Error, Result};

/// A test client for making requests to a Flareon project.
///
/// Useful for End-to-End testing Flareon projects.
#[derive(Debug)]
pub struct Client<S> {
    context: Arc<AppContext>,
    handler: S,
}

impl<S> Client<S>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    #[must_use]
    pub fn new(project: FlareonProject<S>) -> Self {
        let (context, handler) = project.into_context();
        Self {
            context: Arc::new(context),
            handler,
        }
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
        prepare_request(&mut request, self.context.clone());

        poll_fn(|cx| self.handler.poll_ready(cx)).await?;
        self.handler.call(request).await
    }
}

#[derive(Debug, Clone, Default)]
pub struct TestRequestBuilder {
    method: http::Method,
    url: String,
    session: Option<Session>,
    config: Option<Arc<ProjectConfig>>,
    database: Option<Arc<Database>>,
    form_data: Option<Vec<(String, String)>>,
}

impl TestRequestBuilder {
    #[must_use]
    pub fn get(url: &str) -> Self {
        Self {
            method: http::Method::GET,
            url: url.to_string(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn post(url: &str) -> Self {
        Self {
            method: http::Method::POST,
            url: url.to_string(),
            ..Self::default()
        }
    }

    pub fn config(&mut self, config: ProjectConfig) -> &mut Self {
        self.config = Some(Arc::new(config));
        self
    }

    pub fn with_default_config(&mut self) -> &mut Self {
        self.config = Some(Arc::new(ProjectConfig::default()));
        self
    }

    pub fn with_session(&mut self) -> &mut Self {
        let session_store = MemoryStore::default();
        self.session = Some(Session::new(None, Arc::new(session_store), None));
        self
    }

    pub fn with_session_from(&mut self, request: &Request) -> &mut Self {
        self.session = Some(request.session().clone());
        self
    }

    pub fn session(&mut self, session: Session) -> &mut Self {
        self.session = Some(session);
        self
    }

    pub fn database(&mut self, database: Arc<Database>) -> &mut Self {
        self.database = Some(database);
        self
    }

    pub fn with_db_auth(&mut self, db: Arc<Database>) -> &mut Self {
        let auth_backend = DatabaseUserBackend;
        let config = ProjectConfig::builder().auth_backend(auth_backend).build();

        self.with_session();
        self.config(config);
        self.database(db);
        self
    }

    pub fn form_data<T: ToString>(&mut self, form_data: &[(T, T)]) -> &mut Self {
        self.form_data = Some(
            form_data
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        self
    }

    #[must_use]
    pub fn build(&mut self) -> http::Request<Body> {
        let mut request = http::Request::builder()
            .method(self.method.clone())
            .uri(self.url.clone())
            .body(Body::empty())
            .expect("Test request should be valid");

        let app_context = AppContext::new(
            self.config.clone().unwrap_or_default(),
            Vec::new(),
            Arc::new(Router::empty()),
            self.database.clone(),
        );
        prepare_request(&mut request, Arc::new(app_context));

        if let Some(session) = &self.session {
            request.extensions_mut().insert(session.clone());
        }

        if let Some(form_data) = &self.form_data {
            if self.method != http::Method::POST {
                todo!("Form data can currently only be used with POST requests");
            }

            let mut data = form_urlencoded::Serializer::new(String::new());
            for (key, value) in form_data {
                data.append_pair(key, value);
            }

            *request.body_mut() = Body::fixed(data.finish());
            request.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
        }

        request
    }
}

#[derive(Debug)]
pub struct TestDatabaseBuilder {
    migrations: Vec<MigrationWrapper>,
}

impl Default for TestDatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDatabaseBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    #[must_use]
    pub fn add_migrations<T: DynMigration + 'static, V: IntoIterator<Item = T>>(
        mut self,
        migrations: V,
    ) -> Self {
        self.migrations
            .extend(migrations.into_iter().map(MigrationWrapper::new));
        self
    }

    #[must_use]
    pub fn with_auth(self) -> Self {
        self.add_migrations(flareon::auth::db::migrations::MIGRATIONS.to_vec())
    }

    #[must_use]
    pub async fn build(self) -> Database {
        let engine = MigrationEngine::new(self.migrations);
        let database = Database::new("sqlite::memory:").await.unwrap();
        engine.run(&database).await.unwrap();
        database
    }
}
