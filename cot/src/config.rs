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
#[builder(build_fn(skip))]
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
    pub fn auth_backend<T: AuthBackend + 'static>(&mut self, auth_backend: T) -> &mut Self {
        self.auth_backend = Some(Arc::new(auth_backend));
        self
    }

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

#[cfg(feature = "db")]
#[derive(Debug, Clone, Builder)]
pub struct DatabaseConfig {
    #[builder(setter(into))]
    url: String,
}

#[cfg(feature = "db")]
impl DatabaseConfig {
    #[must_use]
    pub fn builder() -> DatabaseConfigBuilder {
        DatabaseConfigBuilder::default()
    }

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
    #[must_use]
    pub fn builder() -> ProjectConfigBuilder {
        ProjectConfigBuilder::default()
    }

    #[must_use]
    pub fn secret_key(&self) -> &SecretKey {
        &self.secret_key
    }

    #[must_use]
    pub fn fallback_secret_keys(&self) -> &[SecretKey] {
        &self.fallback_secret_keys
    }

    #[must_use]
    pub fn auth_backend(&self) -> &dyn AuthBackend {
        &*self.auth_backend
    }

    #[must_use]
    #[cfg(feature = "db")]
    pub fn database_config(&self) -> &DatabaseConfig {
        &self.database_config
    }
}

#[repr(transparent)]
#[derive(Clone)]
pub struct SecretKey(Box<[u8]>);

impl SecretKey {
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
