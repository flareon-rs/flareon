//! Test utilities for Flareon projects.

use std::future::poll_fn;
use std::sync::Arc;

use derive_more::Debug;
use tower::Service;
use tower_sessions::{MemoryStore, Session};

#[cfg(feature = "db")]
use crate::auth::db::DatabaseUserBackend;
use crate::config::ProjectConfig;
#[cfg(feature = "db")]
use crate::db::migrations::{
    DynMigration, MigrationDependency, MigrationEngine, MigrationWrapper, Operation,
};
#[cfg(feature = "db")]
use crate::db::Database;
use crate::request::{Request, RequestExt};
use crate::response::Response;
use crate::router::Router;
use crate::{prepare_request, AppContext, Body, BoxedHandler, FlareonProject, Result};

/// A test client for making requests to a Flareon project.
///
/// Useful for End-to-End testing Flareon projects.
#[derive(Debug)]
pub struct Client {
    context: Arc<AppContext>,
    handler: BoxedHandler,
}

impl Client {
    #[must_use]
    pub fn new(project: FlareonProject) -> Self {
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
    #[cfg(feature = "db")]
    database: Option<Arc<Database>>,
    form_data: Option<Vec<(String, String)>>,
    #[cfg(feature = "json")]
    json_data: Option<String>,
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

    #[cfg(feature = "db")]
    pub fn database(&mut self, database: Arc<Database>) -> &mut Self {
        self.database = Some(database);
        self
    }

    #[cfg(feature = "db")]
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

    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(&mut self, data: &T) -> &mut Self {
        self.json_data = Some(serde_json::to_string(data).expect("Failed to serialize JSON"));
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
            #[cfg(feature = "db")]
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

        #[cfg(feature = "json")]
        if let Some(json_data) = &self.json_data {
            *request.body_mut() = Body::fixed(json_data.clone());
            request.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }

        request
    }
}

#[cfg(feature = "db")]
#[derive(Debug)]
pub struct TestDatabase {
    database: Arc<Database>,
    kind: TestDatabaseKind,
    migrations: Vec<MigrationWrapper>,
}

#[cfg(feature = "db")]
impl TestDatabase {
    fn new(database: Database, kind: TestDatabaseKind) -> TestDatabase {
        Self {
            database: Arc::new(database),
            kind,
            migrations: Vec::new(),
        }
    }

    /// Create a new in-memory SQLite database for testing.
    pub async fn new_sqlite() -> Result<Self> {
        let database = Database::new("sqlite::memory:").await?;
        Ok(Self::new(database, TestDatabaseKind::Sqlite))
    }

    /// Create a new PostgreSQL database for testing and connects to it.
    ///
    /// The database URL is read from the `POSTGRES_URL` environment variable.
    /// Note that it shouldn't include the database name — the function will
    /// create a new database for the test by connecting to the `postgres`
    /// database. If no URL is provided, it defaults to
    /// `postgresql://flareon:flareon@localhost`.
    ///
    /// The database is created with the name `test_flareon__{test_name}`.
    /// Make sure that `test_name` is unique for each test so that the databases
    /// don't conflict with each other.
    ///
    /// The database is dropped when `self.cleanup()` is called. Note that this
    /// means that the database will not be dropped if the test panics.
    pub async fn new_postgres(test_name: &str) -> Result<Self> {
        let db_url = std::env::var("POSTGRES_URL")
            .unwrap_or_else(|_| "postgresql://flareon:flareon@localhost".to_string());
        let database = Database::new(format!("{db_url}/postgres")).await?;

        let test_database_name = format!("test_flareon__{test_name}");
        database
            .raw(&format!("DROP DATABASE IF EXISTS {test_database_name}"))
            .await?;
        database
            .raw(&format!("CREATE DATABASE {test_database_name}"))
            .await?;
        database.close().await?;

        let database = Database::new(format!("{db_url}/{test_database_name}")).await?;

        Ok(Self::new(
            database,
            TestDatabaseKind::Postgres {
                db_url,
                db_name: test_database_name,
            },
        ))
    }

    /// Create a new MySQL database for testing and connects to it.
    ///
    /// The database URL is read from the `MYSQL_URL` environment variable.
    /// Note that it shouldn't include the database name — the function will
    /// create a new database for the test by connecting to the `mysql`
    /// database. If no URL is provided, it defaults to
    /// `mysql://root:@localhost`.
    ///
    /// The database is created with the name `test_flareon__{test_name}`.
    /// Make sure that `test_name` is unique for each test so that the databases
    /// don't conflict with each other.
    ///
    /// The database is dropped when `self.cleanup()` is called. Note that this
    /// means that the database will not be dropped if the test panics.
    pub async fn new_mysql(test_name: &str) -> Result<Self> {
        let db_url =
            std::env::var("MYSQL_URL").unwrap_or_else(|_| "mysql://root:@localhost".to_string());
        let database = Database::new(format!("{db_url}/mysql")).await?;

        let test_database_name = format!("test_flareon__{test_name}");
        database
            .raw(&format!("DROP DATABASE IF EXISTS {test_database_name}"))
            .await?;
        database
            .raw(&format!("CREATE DATABASE {test_database_name}"))
            .await?;
        database.close().await?;

        let database = Database::new(format!("{db_url}/{test_database_name}")).await?;

        Ok(Self::new(
            database,
            TestDatabaseKind::MySql {
                db_url,
                db_name: test_database_name,
            },
        ))
    }

    pub fn add_migrations<T: DynMigration + 'static, V: IntoIterator<Item = T>>(
        &mut self,
        migrations: V,
    ) -> &mut Self {
        self.migrations
            .extend(migrations.into_iter().map(MigrationWrapper::new));
        self
    }

    #[cfg(feature = "db")]
    pub fn with_auth(&mut self) -> &mut Self {
        self.add_migrations(flareon::auth::db::migrations::MIGRATIONS.to_vec());
        self
    }

    pub async fn run_migrations(&mut self) -> &mut Self {
        if !self.migrations.is_empty() {
            let engine = MigrationEngine::new(std::mem::take(&mut self.migrations)).unwrap();
            engine.run(&self.database()).await.unwrap();
        }
        self
    }

    #[must_use]
    pub fn database(&self) -> Arc<Database> {
        self.database.clone()
    }

    pub async fn cleanup(&self) -> Result<()> {
        self.database.close().await?;
        match &self.kind {
            TestDatabaseKind::Sqlite => {}
            TestDatabaseKind::Postgres { db_url, db_name } => {
                let database = Database::new(format!("{db_url}/postgres")).await?;

                database.raw(&format!("DROP DATABASE {db_name}")).await?;
                database.close().await?;
            }
            TestDatabaseKind::MySql { db_url, db_name } => {
                let database = Database::new(format!("{db_url}/mysql")).await?;

                database.raw(&format!("DROP DATABASE {db_name}")).await?;
                database.close().await?;
            }
        }

        Ok(())
    }
}

#[cfg(feature = "db")]
impl std::ops::Deref for TestDatabase {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

#[cfg(feature = "db")]
#[derive(Debug, Clone)]
enum TestDatabaseKind {
    Sqlite,
    Postgres { db_url: String, db_name: String },
    MySql { db_url: String, db_name: String },
}

#[cfg(feature = "db")]
#[derive(Debug, Clone)]
pub struct TestMigration {
    app_name: &'static str,
    name: &'static str,
    dependencies: Vec<MigrationDependency>,
    operations: Vec<Operation>,
}

#[cfg(feature = "db")]
impl TestMigration {
    #[must_use]
    pub fn new<D: Into<Vec<MigrationDependency>>, O: Into<Vec<Operation>>>(
        app_name: &'static str,
        name: &'static str,
        dependencies: D,
        operations: O,
    ) -> Self {
        Self {
            app_name,
            name,
            dependencies: dependencies.into(),
            operations: operations.into(),
        }
    }
}

#[cfg(feature = "db")]
impl DynMigration for TestMigration {
    fn app_name(&self) -> &str {
        self.app_name
    }

    fn name(&self) -> &str {
        self.name
    }

    fn dependencies(&self) -> &[MigrationDependency] {
        &self.dependencies
    }

    fn operations(&self) -> &[Operation] {
        &self.operations
    }
}
