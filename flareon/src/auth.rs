//! Authentication system for Flareon.
//!
//! This module provides the authentication system for Flareon. It includes
//! traits for user objects and backends, as well as password hashing and
//! verification.
//!
//! For the default way to store users in the database, see the [`db`] module.

pub mod db;

use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use flareon::config::SecretKey;
#[cfg(test)]
use mockall::automock;
use password_auth::VerifyError;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use thiserror::Error;

use crate::db::impl_sqlite::SqliteValueRef;
use crate::db::{ColumnType, DatabaseField, FromDbValue, SqlxValueRef, ToDbValue};
use crate::request::{Request, RequestExt};

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Password hash is invalid")]
    PasswordHashInvalid,
    #[error("Error while accessing the session object")]
    SessionsError(#[from] tower_sessions::session::Error),
    #[error("Error while accessing the user object")]
    UserBackendError(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AuthError {
    pub fn backend_error(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::UserBackendError(Box::new(error))
    }
}

pub type Result<T> = std::result::Result<T, AuthError>;

/// A user object that can be authenticated.
///
/// This trait is used to represent a user object that can be authenticated and
/// is a core of the authentication system. A `User` object is returned by
/// [`AuthRequestExt::user()`] and is used to check if a user is authenticated
/// and to access user data. If there is no active user session, the `User`
/// object returned by [`AuthRequestExt::user()`] is an [`AnonymousUser`]
/// object.
///
/// A concrete instance of a `User` object is returned by a backend that
/// implements the [`AuthBackend`] trait. The default backend is the
/// [`DatabaseUserBackend`](db::DatabaseUserBackend), which stores user data in
/// the database using Flareon ORM.
#[cfg_attr(test, automock)]
pub trait User {
    /// Returns the user's ID.
    ///
    /// The ID is used to identify the user in the database or other storage.
    /// Can also be `None` if the user is not authenticated.
    ///
    /// [`AnonymousUser`] always returns `None`.
    fn id(&self) -> Option<UserId> {
        None
    }

    /// Returns the user's username.
    ///
    /// The username can be `None` if the user is not authenticated.
    ///
    /// [`AnonymousUser`] always returns `None`.
    // mockall requires lifetimes to be specified here
    // (see related issue: https://github.com/asomers/mockall/issues/571)
    #[allow(clippy::needless_lifetimes)]
    fn username<'a>(&'a self) -> Option<&'a str> {
        None
    }

    /// Returns whether the user is active.
    ///
    /// An active user is one that has been authenticated and is not banned or
    /// otherwise disabled. In other words, a user can be authenticated but
    /// unable to access the system.
    ///
    /// [`AnonymousUser`] always returns `false`.
    fn is_active(&self) -> bool {
        false
    }

    /// Returns whether the user is authenticated.
    ///
    /// An authenticated user is one that has been logged in and has an active
    /// session.
    ///
    /// [`AnonymousUser`] always returns `false`.
    fn is_authenticated(&self) -> bool {
        false
    }

    /// Returns the user's last login time.
    ///
    /// This is the time when the user last logged in to the system. Can be
    /// [`None`] if the user has never logged in.
    ///
    /// [`AnonymousUser`] always returns [`None`].
    fn last_login(&self) -> Option<DateTime<FixedOffset>> {
        None
    }

    /// Returns the user's join time.
    ///
    /// This is the time when the user joined the system. Can be [`None`] if the
    /// user hasn't been authenticated.
    ///
    /// [`AnonymousUser`] always returns [`None`].
    fn joined(&self) -> Option<DateTime<FixedOffset>> {
        None
    }

    /// Returns the user's session authentication hash.
    ///
    /// This used to verify that the session hash stored in the session
    /// object is valid. If the session hash is not valid, the user is
    /// logged out. For instance,
    /// [`DatabaseUser`](db::DatabaseUser) implements this method
    /// to generate a session hash using the user's password hash.
    /// Therefore, when a user changes their password, the session hash is
    /// also changed, and all their sessions are invalidated.
    ///
    /// The session auth hash should always be the same for the same secret key,
    /// unless something has changed in the user's data that should invalidate
    /// the session (e.g. password change). Moreover, if a user implementation
    /// returns [`Some`] session hash for some secret key A, it should also
    /// return [`Some`] session hash for any other secret key B.
    ///
    /// If this method returns `None`, the session hash is not checked.
    ///
    /// [`AnonymousUser`] always returns `None`.
    #[allow(unused_variables)]
    fn session_auth_hash(&self, secret_key: &SecretKey) -> Option<SessionAuthHash> {
        None
    }
}

/// A user ID that uniquely identifies a user in a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserId {
    Int(i64),
    String(String),
}

impl UserId {
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(id) => Some(*id),
            Self::String(_) => None,
        }
    }

    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Int(_) => None,
            Self::String(id) => Some(id),
        }
    }
}

/// An anonymous, unauthenticated user.
///
/// This is used to represent a user that is not authenticated. It is returned
/// by the [`AuthRequestExt::user()`] method when there is no active user
/// session.
#[derive(Debug, Copy, Clone, Default)]
pub struct AnonymousUser();

impl PartialEq for AnonymousUser {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl User for AnonymousUser {}

/// A session authentication hash.
///
/// This is used to verify that the session hash stored in the session object is
/// valid. If the session hash is not valid, the user is logged out. More
/// details can be found in the [`User::session_auth_hash`] method.
///
/// # Security
///
/// The implementation of the [`PartialEq`] trait for this type is constant-time
/// to prevent timing attacks.
///
/// The implementation of the [`Debug`] trait for this type hides the session
/// auth hash value to prevent it from being leaked in logs or other debug
/// output.
#[repr(transparent)]
#[derive(Clone)]
pub struct SessionAuthHash(Box<[u8]>);

impl SessionAuthHash {
    #[must_use]
    pub fn new(hash: &[u8]) -> Self {
        Self(Box::from(hash))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Box<[u8]> {
        self.0
    }
}

impl From<&[u8]> for SessionAuthHash {
    fn from(hash: &[u8]) -> Self {
        Self::new(hash)
    }
}

impl PartialEq for SessionAuthHash {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for SessionAuthHash {}

impl Debug for SessionAuthHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SessionAuthHash")
            .field(&"**********")
            .finish()
    }
}

/// A hashed password.
///
/// This is used to store a hashed user password in the database. The password
/// hash is created using the `password_auth` crate internally, so by default,
/// it generates the hash using the latest recommended algorithm.
///
/// # Security
///
/// The implementation of the [`Debug`] trait for this type hides the password
/// hash value to prevent it from being leaked in logs or other debug output.
///
/// There is no [`PartialEq`] implementation for this type, as it should never
/// be needed to compare password hashes directly. Instead, use the
/// [`verify`](Self::verify) method to verify a password against the hash.
#[repr(transparent)]
#[derive(Clone)]
pub struct PasswordHash(String);

impl PasswordHash {
    /// Creates a new password hash object from a string.
    ///
    /// Note that this method takes the hash directly. If you need to hash a
    /// password, use [`PasswordHash::from_password`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::auth::{Password, PasswordHash};
    ///
    /// let hash = PasswordHash::from_password(&Password::new("password"));
    /// let stored_hash = hash.into_string();
    /// let hash = PasswordHash::new(stored_hash).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the password hash is invalid.
    pub fn new<T: Into<String>>(hash: T) -> Result<Self> {
        let hash = hash.into();
        password_auth::is_hash_obsolete(&hash).map_err(|_| AuthError::PasswordHashInvalid)?;
        Ok(Self(hash))
    }

    /// Creates a new password hash from a password.
    ///
    /// The password is hashed using the latest recommended algorithm.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::auth::{Password, PasswordHash};
    ///
    /// let hash = PasswordHash::from_password(&Password::new("password"));
    /// ```
    #[must_use]
    pub fn from_password(password: &Password) -> Self {
        let hash = password_auth::generate_hash(password.as_str());
        Self(hash)
    }

    /// Verifies a password against the hash.
    ///
    /// * If the password is valid, returns [`PasswordVerificationResult::Ok`].
    /// * If the password is valid but the hash is obsolete, returns
    ///   [`PasswordVerificationResult::OkObsolete`] with the new hash
    ///   calculated with the currently preferred algorithm. The new hash should
    ///   be saved to the database.
    /// * If the password is invalid, returns
    ///   [`PasswordVerificationResult::Invalid`].
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::auth::{Password, PasswordHash, PasswordVerificationResult};
    ///
    /// let password = Password::new("password");
    /// let hash = PasswordHash::from_password(&password);
    ///
    /// match hash.verify(&password) {
    ///     PasswordVerificationResult::Ok => println!("Password is valid"),
    ///     PasswordVerificationResult::OkObsolete(new_hash) => println!(
    ///         "Password is valid, but the hash is obsolete. Save the new hash: {}",
    ///         new_hash.as_str()
    ///     ),
    ///     PasswordVerificationResult::Invalid => println!("Password is invalid"),
    /// }
    /// ```
    pub fn verify(&self, password: &Password) -> PasswordVerificationResult {
        const VALID_ERROR_STR: &str = "password hash should always be valid if created with `PasswordHash::new` or `PasswordHash::from_password`";

        match password_auth::verify_password(password.as_str(), &self.0) {
            Ok(()) => {
                let is_obsolete = password_auth::is_hash_obsolete(&self.0).expect(VALID_ERROR_STR);
                if is_obsolete {
                    PasswordVerificationResult::OkObsolete(PasswordHash::from_password(password))
                } else {
                    PasswordVerificationResult::Ok
                }
            }
            Err(error) => match error {
                VerifyError::PasswordInvalid => PasswordVerificationResult::Invalid,
                VerifyError::Parse(_) => unreachable!("{VALID_ERROR_STR}"),
            },
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl TryFrom<String> for PasswordHash {
    type Error = AuthError;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub enum PasswordVerificationResult {
    /// The password is valid.
    Ok,
    /// The password is valid, but the hash is obsolete. The new hash calculated
    /// with the currently preferred algorithm is provided, and it should be
    /// saved to the database.
    OkObsolete(PasswordHash),
    /// The password is invalid.
    Invalid,
}

impl Debug for PasswordHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PasswordHash")
            .field(&format!("{}**********", &self.0[..10]))
            .finish()
    }
}

impl DatabaseField for PasswordHash {
    // TODO change to length-limiting type
    const TYPE: ColumnType = ColumnType::Text;
}

impl FromDbValue for PasswordHash {
    fn from_sqlite(value: SqliteValueRef) -> flareon::db::Result<Self> {
        PasswordHash::new(value.get::<String>()?).map_err(flareon::db::DatabaseError::value_decode)
    }
}

impl ToDbValue for PasswordHash {
    fn as_sea_query_value(&self) -> sea_query::Value {
        self.0.clone().into()
    }
}

/// A password.
///
/// It is always recommended to store passwords in memory using this newtype
/// instead of a raw String, as it has a [`Debug`] implementation that hides
/// the password value.
///
/// For persisting passwords in the database, use [`PasswordHash`].
///
/// # Security
///
/// The implementation of the [`Debug`] trait for this type hides the password
/// value to prevent it from being leaked in logs or other debug output.
///
/// # Examples
///
/// ```
/// use flareon::auth::Password;
///
/// let password = Password::new("pass");
/// assert_eq!(&format!("{:?}", password), "Password(\"**********\")");
/// ```
#[derive(Clone)]
pub struct Password(String);

impl Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Password").field(&"**********").finish()
    }
}

impl Password {
    /// Creates a new password object.
    ///
    /// # Examples
    ///
    /// ```
    /// use flareon::auth::Password;
    ///
    /// let password = Password::new("password");
    /// ```
    #[must_use]
    pub fn new<T: Into<String>>(password: T) -> Self {
        Self(password.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<&Password> for Password {
    fn from(password: &Password) -> Self {
        password.clone()
    }
}

impl From<&str> for Password {
    fn from(password: &str) -> Self {
        Self::new(password)
    }
}

impl From<String> for Password {
    fn from(password: String) -> Self {
        Self::new(password)
    }
}

mod private {
    pub trait Sealed {}
}

/// A trait providing some useful authentication methods for the [`Request`]
/// type.
#[async_trait]
pub trait AuthRequestExt: private::Sealed {
    /// Returns the current user.
    ///
    /// This uses the auth backend configured in
    /// [`ProjectConfig::auth_backend`](crate::config::ProjectConfig::auth_backend).
    /// If the user is not authenticated, the [`AnonymousUser`] object is
    /// returned.
    ///
    /// This method caches the user object in the request extensions, so it
    /// doesn't need to be fetched from the backend on every call.
    ///
    /// # Errors
    ///
    /// Returns an error if the user object cannot be fetched from the backend.
    ///
    /// Returns an error if the underlying session backend fails.
    async fn user(&mut self) -> Result<&dyn User>;

    /// Authenticates a user with the given credentials.
    ///
    /// This uses the auth backend configured in
    /// [`ProjectConfig::auth_backend`](crate::config::ProjectConfig::auth_backend).
    /// If the authentication is successful, the user object is returned. If the
    /// authentication fails, [`None`] is returned.
    ///
    /// Note that this doesn't log the user in, it only checks if the
    /// credentials are valid and returns the user object. To log the user
    /// in the current session, use the [`login`](Self::login) method.
    ///
    /// # Errors
    ///
    /// Returns an error if the AuthBackend accepts the credentials but fails
    /// to fetch the user object.
    async fn authenticate(
        &mut self,
        credentials: &(dyn Any + Send + Sync),
    ) -> Result<Option<Box<dyn User + Send + Sync>>>;

    /// Logs in a user.
    ///
    /// This logs in the user in the current session. The user object is stored
    /// in the session object and can be accessed using the [`user`](Self::user)
    /// method.
    ///
    /// # Errors
    ///
    /// Returns an error if the user object cannot be stored in the session
    /// object.
    async fn login(&mut self, user: Box<dyn User + Send + Sync + 'static>) -> Result<()>;

    /// Logs out the current user.
    ///
    /// This removes the user object from the session object and logs the user
    /// out. Subsequent calls to [`user`](Self::user) will return the
    /// [`AnonymousUser`] object, unless a user is logged in again.
    ///
    /// # Errors
    ///
    /// Returns an error if the user object cannot be removed from the session
    /// object.
    async fn logout(&mut self) -> Result<()>;
}

const USER_ID_SESSION_KEY: &str = "__flareon_auth_user_id";
const SESSION_HASH_SESSION_KEY: &str = "__flareon_auth_session_hash";

type UserExtension = Arc<dyn User + Send + Sync + 'static>;

impl private::Sealed for Request {}

#[async_trait]
impl AuthRequestExt for Request {
    async fn user(&mut self) -> Result<&dyn User> {
        if self.extensions().get::<UserExtension>().is_none() {
            if let Some(user) = get_user_with_saved_id(self).await? {
                self.extensions_mut().insert(UserExtension::from(user));
            } else {
                self.logout().await?;
            }
        }

        Ok(&**self
            .extensions()
            .get::<UserExtension>()
            .expect("User extension should have just been added"))
    }

    async fn authenticate(
        &mut self,
        credentials: &(dyn Any + Send + Sync),
    ) -> Result<Option<Box<dyn User + Send + Sync>>> {
        self.project_config()
            .auth_backend()
            .authenticate(self, credentials)
            .await
    }

    async fn login(&mut self, user: Box<dyn User + Send + Sync + 'static>) -> Result<()> {
        let user = UserExtension::from(user);
        if let Some(user_id) = user.id() {
            self.session_mut()
                .insert(USER_ID_SESSION_KEY, user_id)
                .await?;
        }
        let secret_key = self.project_config().secret_key();
        if let Some(session_auth_hash) = user.session_auth_hash(secret_key) {
            self.session_mut()
                .insert(SESSION_HASH_SESSION_KEY, session_auth_hash.as_bytes())
                .await?;
        }
        self.extensions_mut().insert(user);

        Ok(())
    }

    async fn logout(&mut self) -> Result<()> {
        self.session_mut().remove_value(USER_ID_SESSION_KEY).await?;
        self.session_mut()
            .remove_value(SESSION_HASH_SESSION_KEY)
            .await?;
        self.extensions_mut()
            .insert::<UserExtension>(Arc::new(AnonymousUser()));

        Ok(())
    }
}

async fn get_user_with_saved_id(
    request: &mut Request,
) -> Result<Option<Box<dyn User + Send + Sync>>> {
    let Some(user_id) = request.session().get::<UserId>(USER_ID_SESSION_KEY).await? else {
        return Ok(None);
    };

    let Some(user) = request
        .project_config()
        .auth_backend()
        .get_by_id(request, user_id)
        .await?
    else {
        return Ok(None);
    };

    if session_auth_hash_valid(&*user, request).await? {
        Ok(Some(user))
    } else {
        Ok(None)
    }
}

async fn session_auth_hash_valid(
    user: &(dyn User + Send + Sync),
    request: &mut Request,
) -> Result<bool> {
    let config = request.project_config();

    let Some(user_hash) = user.session_auth_hash(config.secret_key()) else {
        return Ok(true);
    };

    let stored_hash = request
        .session()
        .get::<Vec<u8>>(SESSION_HASH_SESSION_KEY)
        .await?
        .expect("Session hash should be present in the session object");
    let stored_hash = SessionAuthHash::new(&stored_hash);

    if user_hash == stored_hash {
        return Ok(true);
    }

    // If the primary secret key doesn't match, try the fallback keys (in other
    // words, check if the session hash was generated with an old secret key)
    // and update the session hash with the new key if a match is found.
    for fallback_key in config.fallback_secret_keys() {
        let user_hash_fallback = user
            .session_auth_hash(fallback_key)
            .expect("User should have a session hash for each secret key");
        if user_hash_fallback == stored_hash {
            request
                .session_mut()
                .insert(SESSION_HASH_SESSION_KEY, user_hash.as_bytes())
                .await?;

            return Ok(true);
        }
    }

    Ok(false)
}

#[async_trait]
pub trait AuthBackend: Send + Sync {
    async fn authenticate(
        &self,
        request: &Request,
        credentials: &(dyn Any + Send + Sync),
    ) -> Result<Option<Box<dyn User + Send + Sync>>>;

    async fn get_by_id(
        &self,
        request: &Request,
        id: UserId,
    ) -> Result<Option<Box<dyn User + Send + Sync>>>;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use mockall::predicate::eq;

    use super::*;
    use crate::config::ProjectConfig;
    use crate::test::TestRequestBuilder;

    struct NoUserAuthBackend;

    #[async_trait]
    impl AuthBackend for NoUserAuthBackend {
        async fn authenticate(
            &self,
            _request: &Request,
            _credentials: &(dyn Any + Send + Sync),
        ) -> Result<Option<Box<dyn User + Send + Sync>>> {
            Ok(None)
        }

        async fn get_by_id(
            &self,
            _request: &Request,
            _id: UserId,
        ) -> Result<Option<Box<dyn User + Send + Sync>>> {
            Ok(None)
        }
    }

    struct MockAuthBackend<F> {
        return_user: F,
    }

    #[async_trait]
    impl<F: Fn() -> MockUser + Send + Sync + 'static> AuthBackend for MockAuthBackend<F> {
        async fn authenticate(
            &self,
            _request: &Request,
            _credentials: &(dyn Any + Send + Sync),
        ) -> Result<Option<Box<dyn User + Send + Sync>>> {
            Ok(Some(Box::new((self.return_user)())))
        }

        async fn get_by_id(
            &self,
            _request: &Request,
            _id: UserId,
        ) -> Result<Option<Box<dyn User + Send + Sync>>> {
            Ok(Some(Box::new((self.return_user)())))
        }
    }

    const TEST_KEY_1: &[u8] = b"key1";
    const TEST_KEY_2: &[u8] = b"key2";
    const TEST_KEY_3: &[u8] = b"key3";

    fn test_request<T: Fn() -> MockUser + Send + Sync + 'static>(return_user: T) -> Request {
        test_request_with_auth_backend(MockAuthBackend { return_user })
    }

    fn test_request_with_auth_backend<T: AuthBackend + 'static>(auth_backend: T) -> Request {
        TestRequestBuilder::get("/")
            .with_session()
            .config(test_project_config(
                auth_backend,
                SecretKey::new(TEST_KEY_1),
                vec![],
            ))
            .build()
    }

    fn test_request_with_config_and_session(
        config: ProjectConfig,
        session_source: &Request,
    ) -> Request {
        TestRequestBuilder::get("/")
            .with_session_from(session_source)
            .config(config)
            .build()
    }

    fn test_project_config<T: AuthBackend + 'static>(
        auth_backend: T,
        secret_key: SecretKey,
        fallback_keys: Vec<SecretKey>,
    ) -> ProjectConfig {
        #[allow(trivial_casts)] // Upcast to the correct Arc type
        ProjectConfig::builder()
            .auth_backend(auth_backend)
            .secret_key(secret_key)
            .fallback_secret_keys(fallback_keys)
            .clone()
            .build()
    }

    #[test]
    fn anonymous_user() {
        let anonymous_user = AnonymousUser();
        assert_eq!(anonymous_user.id(), None);
        assert_eq!(anonymous_user.username(), None);
        assert!(!anonymous_user.is_active());
        assert!(!anonymous_user.is_authenticated());
        assert_eq!(anonymous_user.last_login(), None);
        assert_eq!(anonymous_user.joined(), None);
        assert_eq!(
            anonymous_user.session_auth_hash(&SecretKey::new(b"key")),
            None
        );

        let anonymous_user2 = AnonymousUser();
        assert_eq!(anonymous_user, anonymous_user2);
    }

    #[test]
    fn password_hash() {
        let password = Password::new("password".to_string());
        let hash = PasswordHash::from_password(&password);
        match hash.verify(&password) {
            PasswordVerificationResult::Ok => {}
            _ => panic!("Password hash verification failed"),
        }
    }

    #[test]
    fn session_auth_hash_debug() {
        let hash = SessionAuthHash::from([1, 2, 3].as_ref());
        assert_eq!(format!("{hash:?}"), "SessionAuthHash(\"**********\")");
    }

    #[test]
    fn password_debug() {
        let password = Password::new("password");
        assert_eq!(format!("{password:?}"), "Password(\"**********\")");
    }

    #[test]
    fn password_str() {
        let password = Password::new("password");
        assert_eq!(password.as_str(), "password");
        assert_eq!(password.into_string(), "password");
    }

    const TEST_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$QAAI3EMU1eTLT9NzzBhQjg$khq4zuHsEyk9trGjuqMBFYnTbpqkmn0wXGxFn1nkPBc";

    #[test]
    fn password_hash_debug() {
        let hash = PasswordHash::new(TEST_PASSWORD_HASH).unwrap();
        assert_eq!(
            format!("{hash:?}"),
            "PasswordHash(\"$argon2id$**********\")"
        );
    }

    #[test]
    fn password_hash_verify() {
        let password = Password::new("password");
        let hash = PasswordHash::from_password(&password);
        match hash.verify(&password) {
            PasswordVerificationResult::Ok => {}
            _ => panic!("Password hash verification failed"),
        }

        let wrong_password = Password::new("wrongpassword");
        match hash.verify(&wrong_password) {
            PasswordVerificationResult::Invalid => {}
            _ => panic!("Password hash verification failed"),
        }
    }

    #[test]
    fn password_hash_str() {
        let hash = PasswordHash::new(TEST_PASSWORD_HASH).unwrap();
        assert_eq!(hash.as_str(), TEST_PASSWORD_HASH);
        assert_eq!(hash.into_string(), TEST_PASSWORD_HASH);

        let hash = PasswordHash::try_from(TEST_PASSWORD_HASH.to_string()).unwrap();
        assert_eq!(hash.as_str(), TEST_PASSWORD_HASH);
        assert_eq!(hash.into_string(), TEST_PASSWORD_HASH);
    }

    #[tokio::test]
    async fn user_anonymous() {
        let mut request = test_request_with_auth_backend(NoUserAuthBackend {});

        let user = request.user().await.unwrap();
        assert!(!user.is_authenticated());
        assert!(!user.is_active());
    }

    #[tokio::test]
    async fn user() {
        let mut request = test_request(|| {
            let mut mock_user = MockUser::new();
            mock_user.expect_id().return_const(UserId::Int(1));
            mock_user.expect_session_auth_hash().return_const(None);
            mock_user.expect_username().return_const(Some("mockuser"));
            mock_user
        });

        request
            .session_mut()
            .insert(USER_ID_SESSION_KEY, UserId::Int(1))
            .await
            .unwrap();
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));
    }

    #[tokio::test]
    async fn authenticate() {
        let mut request = test_request(|| {
            let mut mock_user = MockUser::new();
            mock_user.expect_username().return_const(Some("mockuser"));
            mock_user
        });

        let credentials: &(dyn Any + Send + Sync) = &();
        let user = request.authenticate(credentials).await.unwrap().unwrap();
        assert_eq!(user.username(), Some("mockuser"));
    }

    #[tokio::test]
    async fn login_logout() {
        let mut request = test_request(MockUser::new);

        let mut mock_user = MockUser::new();
        mock_user.expect_id().return_const(UserId::Int(1));
        mock_user.expect_session_auth_hash().return_const(None);
        mock_user.expect_username().return_const(Some("mockuser"));

        request.login(Box::new(mock_user)).await.unwrap();
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        request.logout().await.unwrap();
        let user = request.user().await.unwrap();
        assert!(user.username().is_none());
    }

    /// Test that the user is logged out when there is an invalid user ID in the
    /// session (can happen if the user is deleted from the database)
    #[tokio::test]
    async fn logout_on_invalid_user_id_in_session() {
        let mut request = test_request_with_auth_backend(NoUserAuthBackend {});

        request
            .session_mut()
            .insert(USER_ID_SESSION_KEY, UserId::Int(1))
            .await
            .unwrap();

        let user = request.user().await.unwrap();
        assert_eq!(user.username(), None);
        assert!(!user.is_authenticated());
    }

    #[tokio::test]
    async fn logout_on_session_hash_change() {
        let session_auth_hash = Arc::new(Mutex::new(SessionAuthHash::new(&[1, 2, 3])));
        let session_auth_hash_clone = Arc::clone(&session_auth_hash);
        let create_user = move || {
            let session_auth_hash_clone = Arc::clone(&session_auth_hash_clone);
            let mut mock_user = MockUser::new();
            mock_user.expect_id().return_const(UserId::Int(1));
            mock_user
                .expect_session_auth_hash()
                .returning(move |_| Some(session_auth_hash_clone.lock().unwrap().clone()));
            mock_user.expect_username().return_const(Some("mockuser"));
            mock_user
        };

        let mut request = test_request(create_user.clone());

        request.login(Box::new(create_user())).await.unwrap();
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        // Check the user can be retrieved again
        request.extensions_mut().remove::<UserExtension>();
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        // Verify the user is logged out when the session hash changes
        request.extensions_mut().remove::<UserExtension>();
        *session_auth_hash.lock().unwrap() = SessionAuthHash::new(&[4, 5, 6]);
        let user = request.user().await.unwrap();
        assert!(!user.is_authenticated());
        assert_eq!(user.username(), None);
    }

    #[tokio::test]
    async fn user_secret_key_change() {
        let create_user = move || {
            let mut mock_user = MockUser::new();
            mock_user.expect_id().return_const(UserId::Int(1));
            mock_user
                .expect_session_auth_hash()
                .with(eq(SecretKey::new(TEST_KEY_1)))
                .returning(move |_| Some(SessionAuthHash::new(&[1, 2, 3])));
            mock_user
                .expect_session_auth_hash()
                .with(eq(SecretKey::new(TEST_KEY_2)))
                .returning(move |_| Some(SessionAuthHash::new(&[4, 5, 6])));
            mock_user
                .expect_session_auth_hash()
                .with(eq(SecretKey::new(TEST_KEY_3)))
                .returning(move |_| Some(SessionAuthHash::new(&[7, 8, 9])));
            mock_user.expect_username().return_const(Some("mockuser"));
            mock_user
        };

        let mut request = test_request(create_user);

        request.login(Box::new(create_user())).await.unwrap();
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        let replace_keys = move |request: &mut Request, secret_key, fallback_keys| {
            let new_config = test_project_config(
                MockAuthBackend {
                    return_user: create_user,
                },
                secret_key,
                fallback_keys,
            );
            *request = test_request_with_config_and_session(new_config, request);
        };

        // Change the secret key and verify the user is still logged in with the
        // fallback key
        replace_keys(
            &mut request,
            SecretKey::new(TEST_KEY_2),
            vec![SecretKey::new(TEST_KEY_1)],
        );
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        // Remove fallback key and verify the user is still logged in
        replace_keys(&mut request, SecretKey::new(TEST_KEY_2), vec![]);
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), Some("mockuser"));

        // Remove both keys and verify the user is logged out
        replace_keys(&mut request, SecretKey::new(TEST_KEY_3), vec![]);
        let user = request.user().await.unwrap();
        assert_eq!(user.username(), None);
        assert!(!user.is_authenticated());
    }
}
