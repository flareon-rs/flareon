//! Database-backed user authentication backend.
//!
//! This module provides a user type and an authentication backend that stores
//! the user data in a database using the Cot ORM.

use std::any::Any;

use async_trait::async_trait;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha512;
use thiserror::Error;

use crate::admin::{AdminModel, AdminModelManager, DefaultAdminModelManager};
use crate::auth::{
    AuthBackend, AuthError, Password, PasswordHash, PasswordVerificationResult, Result,
    SessionAuthHash, User, UserId,
};
use crate::config::SecretKey;
use crate::db::migrations::SyncDynMigration;
use crate::db::{model, query, Auto, DatabaseBackend, LimitedString, Model};
use crate::request::{Request, RequestExt};
use crate::CotApp;

pub mod migrations;

pub(crate) const MAX_USERNAME_LENGTH: u32 = 255;

/// A user stored in the database.
#[derive(Debug, Clone)]
#[model]
pub struct DatabaseUser {
    id: Auto<i64>,
    #[model(unique)]
    username: LimitedString<MAX_USERNAME_LENGTH>,
    password: PasswordHash,
}

#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum CreateUserError {
    #[error("username is too long (max {MAX_USERNAME_LENGTH} characters, got {0})")]
    UsernameTooLong(usize),
}

impl DatabaseUser {
    #[must_use]
    fn new(
        id: Auto<i64>,
        username: LimitedString<MAX_USERNAME_LENGTH>,
        password: &Password,
    ) -> Self {
        Self {
            id,
            username,
            password: PasswordHash::from_password(password),
        }
    }

    /// Create a new user and save it to the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the user could not be saved to the database.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::auth::db::DatabaseUser;
    /// use cot::auth::Password;
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, StatusCode};
    ///
    /// async fn view(request: &Request) -> cot::Result<Response> {
    ///     let user = DatabaseUser::create_user(
    ///         request.db(),
    ///         "testuser".to_string(),
    ///         &Password::new("password123"),
    ///     )
    ///     .await?;
    ///
    ///     Ok(Response::new_html(
    ///         StatusCode::OK,
    ///         Body::fixed("User created!"),
    ///     ))
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> cot::Result<()> {
    /// #     use cot::test::{TestDatabase, TestRequestBuilder};
    /// #     let mut test_database = TestDatabase::new_sqlite().await?;
    /// #     test_database.with_auth().run_migrations().await;
    /// #     let request = TestRequestBuilder::get("/")
    /// #         .with_db_auth(test_database.database())
    /// #         .build();
    /// #     view(&request).await?;
    /// #     test_database.cleanup().await?;
    /// #     Ok(())
    /// # }
    /// ```
    pub async fn create_user<DB: DatabaseBackend, T: Into<String>, U: Into<Password>>(
        db: &DB,
        username: T,
        password: U,
    ) -> Result<Self> {
        let username = username.into();
        let username_length = username.len();
        let username = LimitedString::<MAX_USERNAME_LENGTH>::new(username).map_err(|_| {
            AuthError::backend_error(CreateUserError::UsernameTooLong(username_length))
        })?;

        let mut user = Self::new(Auto::auto(), username, &password.into());
        user.save(db).await.map_err(AuthError::backend_error)?;

        Ok(user)
    }

    pub async fn get_by_id<DB: DatabaseBackend>(db: &DB, id: UserId) -> Result<Option<Self>> {
        let id = id.as_int().expect("User ID should be an integer");

        let db_user = query!(DatabaseUser, $id == id)
            .get(db)
            .await
            .map_err(AuthError::backend_error)?;

        Ok(db_user)
    }

    pub async fn get_by_username<DB: DatabaseBackend>(
        db: &DB,
        username: &str,
    ) -> Result<Option<Self>> {
        let username = LimitedString::<MAX_USERNAME_LENGTH>::new(username).map_err(|_| {
            AuthError::backend_error(CreateUserError::UsernameTooLong(username.len()))
        })?;
        let db_user = query!(DatabaseUser, $username == username)
            .get(db)
            .await
            .map_err(AuthError::backend_error)?;

        Ok(db_user)
    }

    pub async fn authenticate<DB: DatabaseBackend>(
        db: &DB,
        credentials: &DatabaseUserCredentials,
    ) -> Result<Option<Self>> {
        let username = credentials.username();
        let username_limited = LimitedString::<MAX_USERNAME_LENGTH>::new(username.to_string())
            .map_err(|_| {
                AuthError::backend_error(CreateUserError::UsernameTooLong(username.len()))
            })?;
        let user = query!(DatabaseUser, $username == username_limited)
            .get(db)
            .await
            .map_err(AuthError::backend_error)?;

        if let Some(mut user) = user {
            let password_hash = &user.password;
            match password_hash.verify(credentials.password()) {
                PasswordVerificationResult::Ok => Ok(Some(user)),
                PasswordVerificationResult::OkObsolete(new_hash) => {
                    user.password = new_hash;
                    user.save(db).await.map_err(AuthError::backend_error)?;
                    Ok(Some(user))
                }
                PasswordVerificationResult::Invalid => Ok(None),
            }
        } else {
            // SECURITY: If no user was found, run the same hashing function to prevent
            // timing attacks from being used to determine if a user exists. Additionally,
            // do something with the result to prevent the compiler from optimizing out the
            // operation.
            // TODO: benchmark this to make sure it works as expected
            let dummy_hash = PasswordHash::from_password(credentials.password());
            if let PasswordVerificationResult::Invalid = dummy_hash.verify(credentials.password()) {
                panic!("Password hash verification should never fail for a newly generated hash");
            }
            Ok(None)
        }
    }

    #[must_use]
    pub fn id(&self) -> i64 {
        self.id
            .expect("called DatabaseUser::id() on an unsaved instance")
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }
}

type SessionAuthHmac = Hmac<Sha512>;

impl User for DatabaseUser {
    fn id(&self) -> Option<UserId> {
        Some(UserId::Int(self.id()))
    }

    fn username(&self) -> Option<&str> {
        Some(&self.username)
    }

    fn is_active(&self) -> bool {
        true
    }

    fn is_authenticated(&self) -> bool {
        true
    }

    fn session_auth_hash(&self, secret_key: &SecretKey) -> Option<SessionAuthHash> {
        let mut mac = SessionAuthHmac::new_from_slice(secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(self.password.as_str().as_bytes());
        let hmac_data = mac.finalize().into_bytes();

        Some(SessionAuthHash::new(&hmac_data))
    }
}

#[async_trait]
impl AdminModel for DatabaseUser {
    async fn get_objects(request: &Request) -> crate::Result<Vec<Self>> {
        Ok(DatabaseUser::objects()
            .all(request.db())
            .await
            .map_err(AuthError::backend_error)?)
    }

    fn name() -> &'static str {
        "DatabaseUser"
    }

    fn url_name() -> &'static str {
        "database_user"
    }

    fn display(&self) -> String {
        format!("{self:?}")
    }
}

/// Credentials for authenticating a user stored in the database.
///
/// This struct is used to authenticate a user stored in the database. It
/// contains the username and password of the user.
///
/// Can be passed to
/// [`AuthRequestExt::authenticate`](crate::auth::AuthRequestExt::authenticate)
/// to authenticate a user when using the [`DatabaseUserBackend`].
#[derive(Debug, Clone)]
pub struct DatabaseUserCredentials {
    username: String,
    password: Password,
}

impl DatabaseUserCredentials {
    #[must_use]
    pub fn new(username: String, password: Password) -> Self {
        Self { username, password }
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub fn password(&self) -> &Password {
        &self.password
    }
}

/// The authentication backend for users stored in the database.
///
/// This is the default authentication backend for Cot. It authenticates
/// users stored in the database using the [`DatabaseUser`] model.
///
/// This backend supports authenticating users using the
/// [`DatabaseUserCredentials`] struct and ignores all other credential types.
#[derive(Debug, Copy, Clone)]
pub struct DatabaseUserBackend;

impl Default for DatabaseUserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseUserBackend {
    /// Create a new instance of the database user authentication backend.
    ///
    /// # Example
    ///
    /// ```
    /// use cot::auth::db::DatabaseUserBackend;
    /// use cot::config::ProjectConfig;
    ///
    /// let backend = DatabaseUserBackend::new();
    /// let config = ProjectConfig::builder().auth_backend(backend).build();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AuthBackend for DatabaseUserBackend {
    async fn authenticate(
        &self,
        request: &Request,
        credentials: &(dyn Any + Send + Sync),
    ) -> Result<Option<Box<dyn User + Send + Sync>>> {
        if let Some(credentials) = credentials.downcast_ref::<DatabaseUserCredentials>() {
            #[allow(trivial_casts)] // Upcast to the correct Box type
            Ok(DatabaseUser::authenticate(request.db(), credentials)
                .await
                .map(|user| user.map(|user| Box::new(user) as Box<dyn User + Send + Sync>))?)
        } else {
            Ok(None)
        }
    }

    async fn get_by_id(
        &self,
        request: &Request,
        id: UserId,
    ) -> Result<Option<Box<dyn User + Send + Sync>>> {
        #[allow(trivial_casts)] // Upcast to the correct Box type
        Ok(DatabaseUser::get_by_id(request.db(), id)
            .await?
            .map(|user| Box::new(user) as Box<dyn User + Send + Sync>))
    }
}

#[derive(Debug, Copy, Clone)]
pub struct DatabaseUserApp;

impl Default for DatabaseUserApp {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseUserApp {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl CotApp for DatabaseUserApp {
    fn name(&self) -> &'static str {
        "cot_db_user"
    }

    fn admin_model_managers(&self) -> Vec<Box<dyn AdminModelManager>> {
        vec![Box::new(DefaultAdminModelManager::<DatabaseUser>::new())]
    }

    fn migrations(&self) -> Vec<Box<SyncDynMigration>> {
        // TODO: this is way too complicated for the user-facing API
        #[allow(trivial_casts)]
        migrations::MIGRATIONS
            .iter()
            .copied()
            .map(|x| Box::new(x) as Box<SyncDynMigration>)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretKey;
    use crate::db::MockDatabaseBackend;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn session_auth_hash() {
        let user = DatabaseUser::new(
            Auto::fixed(1),
            LimitedString::new("testuser").unwrap(),
            &Password::new("password123"),
        );
        let secret_key = SecretKey::new(b"supersecretkey");

        let hash = user.session_auth_hash(&secret_key);
        assert!(hash.is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn database_user_traits() {
        let user = DatabaseUser::new(
            Auto::fixed(1),
            LimitedString::new("testuser").unwrap(),
            &Password::new("password123"),
        );
        let user_ref: &dyn User = &user;
        assert_eq!(user_ref.id(), Some(UserId::Int(1)));
        assert_eq!(user_ref.username(), Some("testuser"));
        assert!(user_ref.is_active());
        assert!(user_ref.is_authenticated());
        assert!(user_ref
            .session_auth_hash(&SecretKey::new(b"supersecretkey"))
            .is_some());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn create_user() {
        let mut mock_db = MockDatabaseBackend::new();
        mock_db
            .expect_insert::<DatabaseUser>()
            .returning(|_| Ok(()));

        let username = "testuser".to_string();
        let password = Password::new("password123");

        let user = DatabaseUser::create_user(&mock_db, username.clone(), &password)
            .await
            .unwrap();
        assert_eq!(user.username(), username);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn get_by_id() {
        let mut mock_db = MockDatabaseBackend::new();
        let user = DatabaseUser::new(
            Auto::fixed(1),
            LimitedString::new("testuser").unwrap(),
            &Password::new("password123"),
        );

        mock_db
            .expect_get::<DatabaseUser>()
            .returning(move |_| Ok(Some(user.clone())));

        let result = DatabaseUser::get_by_id(&mock_db, UserId::Int(1))
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), "testuser");
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn authenticate() {
        let mut mock_db = MockDatabaseBackend::new();
        let user = DatabaseUser::new(
            Auto::fixed(1),
            LimitedString::new("testuser").unwrap(),
            &Password::new("password123"),
        );

        mock_db
            .expect_get::<DatabaseUser>()
            .returning(move |_| Ok(Some(user.clone())));

        let credentials =
            DatabaseUserCredentials::new("testuser".to_string(), Password::new("password123"));
        let result = DatabaseUser::authenticate(&mock_db, &credentials)
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), "testuser");
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn authenticate_non_existing() {
        let mut mock_db = MockDatabaseBackend::new();

        mock_db
            .expect_get::<DatabaseUser>()
            .returning(move |_| Ok(None));

        let credentials =
            DatabaseUserCredentials::new("testuser".to_string(), Password::new("password123"));
        let result = DatabaseUser::authenticate(&mock_db, &credentials)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn authenticate_invalid_password() {
        let mut mock_db = MockDatabaseBackend::new();
        let user = DatabaseUser::new(
            Auto::fixed(1),
            LimitedString::new("testuser").unwrap(),
            &Password::new("password123"),
        );

        mock_db
            .expect_get::<DatabaseUser>()
            .returning(move |_| Ok(Some(user.clone())));

        let credentials =
            DatabaseUserCredentials::new("testuser".to_string(), Password::new("invalid"));
        let result = DatabaseUser::authenticate(&mock_db, &credentials)
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
