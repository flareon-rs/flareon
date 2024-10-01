//! Database-backed user authentication backend.
//!
//! This module provides a user type and an authentication backend that stores
//! the user data in a database using the Flareon ORM.

use std::any::Any;

use async_trait::async_trait;
use flareon_macros::model;
use hmac::{KeyInit, Mac};

use crate::auth::{
    AuthBackend, AuthError, Password, PasswordHash, PasswordVerificationResult, Result,
    SessionAuthHash, SessionAuthHmac, User, UserId,
};
use crate::config::SecretKey;
use crate::db::{query, DatabaseBackend, Model};
use crate::request::{Request, RequestExt};

mod migrations;

/// A user stored in the database.
#[derive(Debug, Clone)]
#[model]
pub struct DatabaseUser {
    id: i64,
    username: String,
    password: PasswordHash,
}

impl DatabaseUser {
    #[must_use]
    pub fn new(id: i64, username: String, password: &Password) -> Self {
        Self {
            id,
            username,
            password: PasswordHash::from_password(password),
        }
    }

    pub async fn create_user<DB: DatabaseBackend>(
        db: &DB,
        username: String,
        password: &Password,
    ) -> Result<Self> {
        let mut user = Self::new(0, username, password);
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

    pub async fn authenticate<DB: DatabaseBackend>(
        db: &DB,
        credentials: &DatabaseUserCredentials,
    ) -> Result<Option<Self>> {
        let user = query!(DatabaseUser, $username == credentials.username())
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
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }
}

impl User for DatabaseUser {
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Copy, Clone)]
pub struct DatabaseUserBackend {}

impl Default for DatabaseUserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseUserBackend {
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
            #[allow(trivial_casts)] // Downcast to the correct Box type
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
        #[allow(trivial_casts)] // Downcast to the correct Box type
        Ok(DatabaseUser::get_by_id(request.db(), id)
            .await?
            .map(|user| Box::new(user) as Box<dyn User + Send + Sync>))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretKey;
    use crate::db::MockDatabaseBackend;

    #[test]
    fn test_session_auth_hash() {
        let user = DatabaseUser::new(1, "testuser".to_string(), &Password::new("password123"));
        let secret_key = SecretKey::new(b"supersecretkey");

        let hash = user.session_auth_hash(&secret_key);
        assert!(hash.is_some());
    }

    #[tokio::test]
    async fn test_create_user() {
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
    async fn test_get_by_id() {
        let mut mock_db = MockDatabaseBackend::new();
        let user = DatabaseUser::new(1, "testuser".to_string(), &Password::new("password123"));

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
    async fn test_authenticate() {
        let mut mock_db = MockDatabaseBackend::new();
        let user = DatabaseUser::new(1, "testuser".to_string(), &Password::new("password123"));

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
    async fn test_authenticate_non_existing() {
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
}
