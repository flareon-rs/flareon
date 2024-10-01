use std::sync::Arc;

use derive_builder::Builder;
use derive_more::Debug;
use subtle::ConstantTimeEq;

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
    #[builder(setter(into))]
    auth_backend: Arc<dyn AuthBackend>,
}

impl ProjectConfigBuilder {
    #[must_use]
    pub fn build(self) -> ProjectConfig {
        ProjectConfig {
            secret_key: self.secret_key.unwrap_or_default(),
            fallback_secret_keys: self.fallback_secret_keys.unwrap_or_default(),
            auth_backend: self.auth_backend.unwrap_or_else(default_auth_backend),
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig::builder().build()
    }
}

fn default_auth_backend() -> Arc<dyn AuthBackend> {
    Arc::new(DatabaseUserBackend::new())
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
        f.debug_tuple("SecretKey").field(&"**********").finish()
    }
}

impl Default for SecretKey {
    fn default() -> Self {
        Self::new(&[])
    }
}
