//! Configuration data for the project.
//!
//! This module contains the configuration data for the project. This includes
//! stuff such as the secret key used for signing cookies, database connection
//! settings, whether the debug mode is enabled, and other project-specific
//! configuration data.
//!
//! The main struct in this module is [`ProjectConfig`], which contains all the
//! configuration data for the project. After creating an instance using
//! [`ProjectConfigBuilder`], it can be passed to the
//! [`CotProject`](crate::CotProject).

use std::sync::Arc;

use derive_builder::Builder;
use derive_more::Debug;
use subtle::ConstantTimeEq;

#[cfg(feature = "db")]
use crate::auth::db::DatabaseUserBackend;
use crate::auth::AuthBackend;

/// Debug mode flag
///
/// This enables some expensive operations that are useful for debugging, such
/// as logging additional information, and collecting some extra diagnostics
/// for generating error pages. This hurts the performance, so it should be
/// disabled for production.
///
/// This is `true` when the application is compiled in debug mode, and `false`
/// when it is compiled in release mode.
pub(crate) const DEBUG_MODE: bool = cfg!(debug_assertions);

/// Whether to display a nice, verbose error page when an error, panic, or
/// 404 "Not Found" occurs.
pub(crate) const DISPLAY_ERROR_PAGE: bool = DEBUG_MODE;

pub(crate) const REGISTER_PANIC_HOOK: bool = true;

/// The configuration for a project.
///
/// This is all the project-specific configuration data that can (and makes
/// sense to) be expressed in a TOML configuration file.
#[derive(Debug, Clone, Builder)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
pub struct ProjectConfig {
    /// The secret key used for signing cookies and other sensitive data. This
    /// is a cryptographic key, should be kept secret, and should a set to a
    /// random and unique value for each project.
    ///
    /// When you want to rotate the secret key, you can move the current key to
    /// the `fallback_secret_keys` list, and set a new key here. Eventually, you
    /// can remove the old key from the list.
    secret_key: SecretKey,
    /// Fallback secret keys that can be used to verify old sessions.
    ///
    /// This is useful for key rotation, where you can add a new key, gradually
    /// migrate sessions to the new key, and then remove the old key.
    fallback_secret_keys: Vec<SecretKey>,
    /// The authentication backend to use.
    ///
    /// This is the backend that is used to authenticate users. The default is
    /// the database backend, which stores user data in the database.
    #[debug("..")]
    #[builder(setter(custom))]
    auth_backend: Arc<dyn AuthBackend>,
    #[cfg(feature = "db")]
    database_config: DatabaseConfig,
}

impl ProjectConfigBuilder {
    /// Sets the authentication backend to use.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::auth::NoAuthBackend;
    /// use cot::config::ProjectConfig;
    ///
    /// let config = ProjectConfig::builder().auth_backend(NoAuthBackend).build();
    /// ```
    pub fn auth_backend<T: AuthBackend + 'static>(&mut self, auth_backend: T) -> &mut Self {
        self.auth_backend = Some(Arc::new(auth_backend));
        self
    }

    /// Builds the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    ///
    /// let config = ProjectConfig::builder().build();
    /// ```
    #[must_use]
    pub fn build(&self) -> ProjectConfig {
        ProjectConfig {
            secret_key: self.secret_key.clone().unwrap_or_default(),
            fallback_secret_keys: self.fallback_secret_keys.clone().unwrap_or_default(),
            auth_backend: self
                .auth_backend
                .clone()
                .unwrap_or_else(default_auth_backend),
            #[cfg(feature = "db")]
            database_config: self.database_config.clone().unwrap_or_default(),
        }
    }
}

/// The configuration for the database.
///
/// This is used to configure the database connection. It's useful as part of
/// the [`ProjectConfig`] struct.
///
/// # Examples
///
/// ```
/// use cot::config::DatabaseConfig;
///
/// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
/// ```
#[cfg(feature = "db")]
#[derive(Debug, Clone, Builder)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
pub struct DatabaseConfig {
    #[builder(setter(into))]
    url: String,
}

#[cfg(feature = "db")]
impl DatabaseConfigBuilder {
    /// Builds the database configuration.
    ///
    /// # Panics
    ///
    /// This will panic if the database URL is not set.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::DatabaseConfig;
    ///
    /// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
    /// ```
    #[must_use]
    pub fn build(&self) -> DatabaseConfig {
        DatabaseConfig {
            url: self.url.clone().expect("Database URL is required"),
        }
    }
}

#[cfg(feature = "db")]
impl DatabaseConfig {
    /// Create a new [`DatabaseConfigBuilder`] to build a [`DatabaseConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::DatabaseConfig;
    ///
    /// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
    /// ```
    #[must_use]
    pub fn builder() -> DatabaseConfigBuilder {
        DatabaseConfigBuilder::default()
    }

    /// Get the URL stored in the database config.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::DatabaseConfig;
    ///
    /// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
    /// assert_eq!(config.url(), "sqlite::memory:");
    /// ```
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }
}

#[cfg(feature = "db")]
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite::memory:".to_string(),
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig::builder().build()
    }
}

fn default_auth_backend() -> Arc<dyn AuthBackend> {
    #[cfg(feature = "db")]
    {
        Arc::new(DatabaseUserBackend::new())
    }

    #[cfg(not(any(feature = "sqlite", feature = "postgres", feature = "mysql")))]
    {
        Arc::new(cot::auth::NoAuthBackend)
    }
}

impl ProjectConfig {
    /// Create a new [`ProjectConfigBuilder`] to build a [`ProjectConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    ///
    /// let config = ProjectConfig::builder().build();
    /// ```
    #[must_use]
    pub fn builder() -> ProjectConfigBuilder {
        ProjectConfigBuilder::default()
    }

    /// Get the secret key stored in the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::builder()
    ///     .secret_key(SecretKey::new(&[1, 2, 3]))
    ///     .build();
    /// assert_eq!(config.secret_key().as_bytes(), &[1, 2, 3]);
    /// ```
    #[must_use]
    pub fn secret_key(&self) -> &SecretKey {
        &self.secret_key
    }

    /// Get the fallback secret keys stored in the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::builder()
    ///     .secret_key(SecretKey::new(&[1, 2, 3]))
    ///     .fallback_secret_keys(vec![SecretKey::new(&[4, 5, 6])])
    ///     .build();
    /// assert_eq!(config.fallback_secret_keys(), &[SecretKey::new(&[4, 5, 6])]);
    /// ```
    #[must_use]
    pub fn fallback_secret_keys(&self) -> &[SecretKey] {
        &self.fallback_secret_keys
    }

    /// Get the authentication backend stored in the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::auth::UserId;
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// fn index(request: Request) -> cot::Result<Response> {
    ///     let user = request
    ///         .project_config()
    ///         .auth_backend()
    ///         .get_by_id(&request, UserId::Int(123));
    ///
    ///     // ... do something with the user
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    pub fn auth_backend(&self) -> &dyn AuthBackend {
        &*self.auth_backend
    }

    /// Get the database configuration stored in the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    ///
    /// let config = ProjectConfig::builder().build();
    /// assert_eq!(config.database_config().url(), "sqlite::memory:");
    /// ```
    #[must_use]
    #[cfg(feature = "db")]
    pub fn database_config(&self) -> &DatabaseConfig {
        &self.database_config
    }
}

/// A secret key.
///
/// This is a wrapper over a byte array, which is used to store a cryptographic
/// key. This is useful for [`ProjectConfig::secret_key`] and
/// [`ProjectConfig::fallback_secret_keys`], which are used to sign cookies and
/// other sensitive data.
///
/// # Security
///
/// The implementation of the [`PartialEq`] trait for this type is constant-time
/// to prevent timing attacks.
///
/// The implementation of the [`Debug`] trait for this type hides the secret key
/// to prevent it from being leaked in logs or other debug output.
///
/// # Examples
///
/// ```
/// use cot::config::SecretKey;
///
/// let key = SecretKey::new(&[1, 2, 3]);
/// assert_eq!(key.as_bytes(), &[1, 2, 3]);
/// ```
#[repr(transparent)]
#[derive(Clone)]
pub struct SecretKey(Box<[u8]>);

impl SecretKey {
    /// Create a new [`SecretKey`] from a byte array.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::SecretKey;
    ///
    /// let key = SecretKey::new(&[1, 2, 3]);
    /// assert_eq!(key.as_bytes(), &[1, 2, 3]);
    /// ```
    #[must_use]
    pub fn new(hash: &[u8]) -> Self {
        Self(Box::from(hash))
    }

    /// Get the byte array stored in the [`SecretKey`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::SecretKey;
    ///
    /// let key = SecretKey::new(&[1, 2, 3]);
    /// assert_eq!(key.as_bytes(), &[1, 2, 3]);
    /// ```
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the [`SecretKey`] and return the byte array stored in it.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::SecretKey;
    ///
    /// let key = SecretKey::new(&[1, 2, 3]);
    /// assert_eq!(key.into_bytes(), Box::from([1, 2, 3]));
    /// ```
    #[must_use]
    pub fn into_bytes(self) -> Box<[u8]> {
        self.0
    }
}

impl From<&[u8]> for SecretKey {
    fn from(hash: &[u8]) -> Self {
        Self::new(hash)
    }
}

impl PartialEq for SecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for SecretKey {}

impl Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // f.debug_tuple("SecretKey").field(&"**********").finish()
        f.debug_tuple("SecretKey").field(&self.0).finish()
    }
}

impl Default for SecretKey {
    fn default() -> Self {
        Self::new(&[])
    }
}
