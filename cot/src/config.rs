//! Configuration data for the project.
//!
//! This module contains the configuration data for the project. This includes
//! stuff such as the secret key used for signing cookies, database connection
//! settings, whether the debug mode is enabled, and other project-specific
//! configuration data.
//!
//! The main struct in this module is [`ProjectConfig`], which contains all the
//! configuration data for the project. After creating an instance using
//! [`ProjectConfig::from_toml`] or [`ProjectConfigBuilder`], it can be passed
//! to the [`Bootstrapper`](crate::project::Bootstrapper).

// most of the config structures might be extended with non-Copy types
// in the future, so to avoid breaking backwards compatibility, we're
// not implementing Copy for them
#![allow(missing_copy_implementations)]

use derive_builder::Builder;
use derive_more::{Debug, From};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

/// The configuration for a project.
///
/// This is all the project-specific configuration data that can (and makes
/// sense to) be expressed in a TOML configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Builder, Serialize, Deserialize)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
#[serde(default)]
pub struct ProjectConfig {
    /// Debug mode flag.
    ///
    /// This enables some expensive operations that are useful for debugging,
    /// such as logging additional information, and collecting some extra
    /// diagnostics for generating error pages. The debug flag also controls
    /// whether Cot displays nice error pages. All of this hurts the
    /// performance, so it should be disabled for production.
    ///
    /// `ProjectConfig::default()` returns `true` here when the application is
    /// compiled in debug mode, and `false` when it is compiled in release
    /// mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// debug = true
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.debug, true);
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub debug: bool,
    /// Whether to register a panic hook.
    ///
    /// The panic hook is used to display information about panics in the Cot
    /// error pages that are displayed in debug mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// register_panic_hook = false
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.register_panic_hook, false);
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub register_panic_hook: bool,
    /// The secret key used for signing cookies and other sensitive data. This
    /// is a cryptographic key, should be kept secret, and should a set to a
    /// random and unique value for each project.
    ///
    /// When you want to rotate the secret key, you can move the current key to
    /// the `fallback_secret_keys` list, and set a new key here. Eventually, you
    /// can remove the old key from the list.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// secret_key = "123abc"
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.secret_key, SecretKey::from("123abc"));
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub secret_key: SecretKey,
    /// Fallback secret keys that can be used to verify old sessions.
    ///
    /// This is useful for key rotation, where you can add a new key, gradually
    /// migrate sessions to the new key, and then remove the old key.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, ProjectConfig, SecretKey};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// fallback_secret_keys = ["123abc"]
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.fallback_secret_keys, vec![SecretKey::from("123abc")]);
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub fallback_secret_keys: Vec<SecretKey>,
    /// The authentication backend to use.
    ///
    /// This is the backend that is used to authenticate users. The default is
    /// the database backend, which stores user data in the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, ProjectConfig};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// [auth_backend]
    /// type = "none"
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.auth_backend, AuthBackendConfig::None);
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub auth_backend: AuthBackendConfig,
    /// Configuration related to the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{AuthBackendConfig, DatabaseUrl, ProjectConfig};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// [database]
    /// url = "sqlite::memory:"
    /// "#,
    /// )?;
    ///
    /// assert_eq!(
    ///     config.database.url,
    ///     Some(DatabaseUrl::from("sqlite::memory:"))
    /// );
    /// # Ok::<(), cot::Error>(())
    /// ```
    #[cfg(feature = "db")]
    pub database: DatabaseConfig,
    /// Configuration related to the middlewares.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{MiddlewareConfig, ProjectConfig};
    ///
    /// let config = ProjectConfig::from_toml(
    ///     r#"
    /// [middlewares]
    /// live_reload.enabled = true
    /// "#,
    /// )?;
    ///
    /// assert_eq!(config.middlewares.live_reload.enabled, true);
    /// # Ok::<(), cot::Error>(())
    /// ```
    pub middlewares: MiddlewareConfig,
}

const fn default_debug() -> bool {
    cfg!(debug_assertions)
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig::builder().build()
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

    /// Create a new [`ProjectConfig`] with the default values for development.
    ///
    /// This is useful for development purposes, where you want to have a
    /// configuration that you can just run as quickly as possible. This is
    /// mainly useful for tests and other things that are run in the local
    /// environment.
    ///
    /// Note that what this function returns exactly is not guaranteed to be
    /// the same across different versions of Cot. It's meant to be used as a
    /// starting point for your development configuration and is subject to
    /// change.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    ///
    /// let config = ProjectConfig::dev_default();
    /// ```
    #[must_use]
    pub fn dev_default() -> ProjectConfig {
        ProjectConfig::builder()
            .debug(true)
            .register_panic_hook(true)
            .build()
    }

    /// Create a new [`ProjectConfig`] from a TOML string.
    ///
    /// # Errors
    ///
    /// This function will return an error if the TOML fails to parse as a
    /// [`ProjectConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::ProjectConfig;
    ///
    /// let toml = r#"
    ///    secret_key = "123abc"
    /// "#;
    /// let config = ProjectConfig::from_toml(toml)?;
    /// # Ok::<_, cot::Error>(())
    /// ```
    pub fn from_toml(toml_content: &str) -> crate::Result<ProjectConfig> {
        let config: ProjectConfig = toml::from_str(toml_content)?;
        Ok(config)
    }
}

impl ProjectConfigBuilder {
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
        let debug = self.debug.unwrap_or(default_debug());
        ProjectConfig {
            debug,
            register_panic_hook: self.register_panic_hook.unwrap_or(true),
            secret_key: self.secret_key.clone().unwrap_or_default(),
            fallback_secret_keys: self.fallback_secret_keys.clone().unwrap_or_default(),
            auth_backend: self.auth_backend.unwrap_or_default(),
            #[cfg(feature = "db")]
            database: self.database.clone().unwrap_or_default(),
            middlewares: self.middlewares.clone().unwrap_or_default(),
        }
    }
}

/// The configuration for the authentication backend.
///
/// # Examples
///
/// ```
/// use cot::config::AuthBackendConfig;
///
/// let config = AuthBackendConfig::Database;
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthBackendConfig {
    /// No authentication backend.
    ///
    /// This enables [`NoAuthBackend`](cot::auth::NoAuthBackend) to be used as
    /// the authentication backend, which effectively disables
    /// authentication.
    None,
    /// Database authentication backend.
    ///
    /// This enables [`DatabaseUserBackend`](cot::auth::db::DatabaseUserBackend)
    /// to be used as the authentication backend.
    #[cfg(feature = "db")]
    Database,
}

impl Default for AuthBackendConfig {
    fn default() -> Self {
        #[cfg(feature = "db")]
        {
            Self::Database
        }

        #[cfg(not(feature = "db"))]
        {
            Self::None
        }
    }
}

/// The configuration for the database.
///
/// It is used as part of the [`ProjectConfig`] struct.
///
/// # Examples
///
/// ```
/// use cot::config::DatabaseConfig;
///
/// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
/// ```
#[cfg(feature = "db")]
#[derive(Debug, Default, Clone, PartialEq, Eq, Builder, Serialize, Deserialize)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
#[serde(default)]
pub struct DatabaseConfig {
    /// The URL of the database, possibly with username, password, and other
    /// options.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{DatabaseConfig, DatabaseUrl};
    ///
    /// let config = DatabaseConfig::builder().url("sqlite::memory:").build();
    /// assert_eq!(config.url, Some(DatabaseUrl::from("sqlite::memory:")));
    /// ```
    #[builder(setter(into, strip_option), default)]
    pub url: Option<DatabaseUrl>,
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
}

/// The configuration for the middlewares.
///
/// This is used as part of the [`ProjectConfig`] struct.
///
/// # Examples
///
/// ```
/// use cot::config::{LiveReloadMiddlewareConfig, MiddlewareConfig};
///
/// let config = MiddlewareConfig::builder()
///     .live_reload(LiveReloadMiddlewareConfig::builder().enabled(true).build())
///     .build();
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Builder, Serialize, Deserialize)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
#[serde(default)]
pub struct MiddlewareConfig {
    /// The configuration for the live reload middleware.
    pub live_reload: LiveReloadMiddlewareConfig,
}

impl MiddlewareConfig {
    /// Create a new [`MiddlewareConfigBuilder`] to build a
    /// [`MiddlewareConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::MiddlewareConfig;
    ///
    /// let config = MiddlewareConfig::builder().build();
    /// ```
    #[must_use]
    pub fn builder() -> MiddlewareConfigBuilder {
        MiddlewareConfigBuilder::default()
    }
}

impl MiddlewareConfigBuilder {
    /// Builds the middleware configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::{LiveReloadMiddlewareConfig, MiddlewareConfig};
    ///
    /// let config = MiddlewareConfig::builder()
    ///     .live_reload(LiveReloadMiddlewareConfig::builder().enabled(true).build())
    ///     .build();
    /// ```
    #[must_use]
    pub fn build(&self) -> MiddlewareConfig {
        MiddlewareConfig {
            live_reload: self.live_reload.clone().unwrap_or_default(),
        }
    }
}

/// The configuration for the live reload middleware.
///
/// This is used as part of the [`MiddlewareConfig`] struct.
///
/// # Examples
///
/// ```
/// use cot::config::LiveReloadMiddlewareConfig;
///
/// let config = LiveReloadMiddlewareConfig::builder().enabled(true).build();
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Builder, Serialize, Deserialize)]
#[builder(build_fn(skip, error = std::convert::Infallible))]
#[serde(default)]
pub struct LiveReloadMiddlewareConfig {
    /// Whether the live reload middleware is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::LiveReloadMiddlewareConfig;
    ///
    /// let config = LiveReloadMiddlewareConfig::builder().enabled(true).build();
    /// ```
    pub enabled: bool,
}

impl LiveReloadMiddlewareConfig {
    /// Create a new [`LiveReloadMiddlewareConfigBuilder`] to build a
    /// [`LiveReloadMiddlewareConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::LiveReloadMiddlewareConfig;
    ///
    /// let config = LiveReloadMiddlewareConfig::builder().build();
    /// ```
    #[must_use]
    pub fn builder() -> LiveReloadMiddlewareConfigBuilder {
        LiveReloadMiddlewareConfigBuilder::default()
    }
}

impl LiveReloadMiddlewareConfigBuilder {
    /// Builds the live reload middleware configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::LiveReloadMiddlewareConfig;
    ///
    /// let config = LiveReloadMiddlewareConfig::builder().enabled(true).build();
    /// ```
    #[must_use]
    pub fn build(&self) -> LiveReloadMiddlewareConfig {
        LiveReloadMiddlewareConfig {
            enabled: self.enabled.unwrap_or_default(),
        }
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "String")]
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
    fn from(value: &[u8]) -> Self {
        Self::new(value)
    }
}

impl From<String> for SecretKey {
    fn from(value: String) -> Self {
        Self::new(value.as_bytes())
    }
}

impl From<&str> for SecretKey {
    fn from(value: &str) -> Self {
        Self::new(value.as_bytes())
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
        // write in single line, regardless whether alternate mode was used or not
        write!(f, "SecretKey(\"**********\")")
    }
}

impl Default for SecretKey {
    fn default() -> Self {
        Self::new(&[])
    }
}

/// A URL for the database.
///
/// This is a wrapper over the [`url::Url`] type, which is used to store the
/// URL of the database. It parses the URL and ensures that it is valid.
///
/// # Security
///
/// The implementation of the [`Debug`] trait for this type hides the password
/// from the debug output.
///
/// # Examples
///
/// ```
/// use cot::config::DatabaseUrl;
///
/// let url = DatabaseUrl::from("postgres://user:password@localhost:5432/database");
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg(feature = "db")]
pub struct DatabaseUrl(url::Url);

#[cfg(feature = "db")]
impl DatabaseUrl {
    /// Returns the string representation of the database URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::config::DatabaseUrl;
    ///
    /// let url = DatabaseUrl::from("postgres://user:password@localhost:5432/database");
    /// assert_eq!(
    ///     url.as_str(),
    ///     "postgres://user:password@localhost:5432/database"
    /// );
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(feature = "db")]
impl From<String> for DatabaseUrl {
    fn from(url: String) -> Self {
        Self(url::Url::parse(&url).expect("valid URL"))
    }
}

#[cfg(feature = "db")]
impl From<&str> for DatabaseUrl {
    fn from(url: &str) -> Self {
        Self(url::Url::parse(url).expect("valid URL"))
    }
}

#[cfg(feature = "db")]
impl Debug for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut new_url = self.0.clone();
        if !new_url.username().is_empty() {
            new_url
                .set_username("********")
                .expect("set_username should succeed if username is present");
        }
        if new_url.password().is_some() {
            new_url
                .set_password(Some("********"))
                .expect("set_password should succeed if password is present");
        }

        f.debug_tuple("DatabaseUrl")
            .field(&new_url.as_str())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_toml_valid() {
        let toml_content = r#"
            debug = true
            register_panic_hook = true
            secret_key = "123abc"
            fallback_secret_keys = ["456def", "789ghi"]
            auth_backend = { type = "none" }
        "#;

        let config = ProjectConfig::from_toml(toml_content).unwrap();

        assert!(config.debug);
        assert!(config.register_panic_hook);
        assert_eq!(config.secret_key.as_bytes(), b"123abc");
        assert_eq!(config.fallback_secret_keys.len(), 2);
        assert_eq!(config.fallback_secret_keys[0].as_bytes(), b"456def");
        assert_eq!(config.fallback_secret_keys[1].as_bytes(), b"789ghi");
        assert_eq!(config.auth_backend, AuthBackendConfig::None);
    }

    #[test]
    fn from_toml_invalid() {
        let toml_content = r"
            debug = true
            secret_key = 123abc
        ";

        let result = ProjectConfig::from_toml(toml_content);
        assert!(result.is_err());
    }

    #[test]
    fn from_toml_missing_fields() {
        let toml_content = r#"
            secret_key = "123abc"
        "#;

        let config = ProjectConfig::from_toml(toml_content).unwrap();
        assert_eq!(config.debug, cfg!(debug_assertions));
        assert_eq!(config.secret_key.as_bytes(), b"123abc");
    }
}
