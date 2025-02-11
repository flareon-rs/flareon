pub(crate) mod backtrace;

use std::fmt::Display;

use derive_more::Debug;
use thiserror::Error;

// Need to rename Backtrace to CotBacktrace, because otherwise it triggers special behavior
// in thiserror library
use crate::error::backtrace::{Backtrace as CotBacktrace, __cot_create_backtrace};

/// An error that can occur while using Cot.
#[derive(Debug)]
pub struct Error {
    pub(crate) inner: ErrorRepr,
    #[debug(skip)]
    backtrace: CotBacktrace,
}

impl Error {
    #[must_use]
    pub(crate) fn new(inner: ErrorRepr) -> Self {
        Self {
            inner,
            backtrace: __cot_create_backtrace(),
        }
    }

    /// Create a new error with a custom error message.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Error;
    ///
    /// let error = Error::custom("An error occurred");
    /// ```
    #[must_use]
    pub fn custom<E>(error: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::new(ErrorRepr::Custom(error.into()))
    }

    /// Create a new "404 Not Found" error without a message.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Error;
    ///
    /// let error = Error::not_found();
    /// ```
    #[must_use]
    pub fn not_found() -> Self {
        Self::new(ErrorRepr::NotFound { message: None })
    }

    /// Create a new "404 Not Found" error with a message.
    ///
    /// Note that the message is only displayed when Cot's debug mode is
    /// enabled. It will not be exposed to the user in production.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::Error;
    ///
    /// let id = 123;
    /// let error = Error::not_found_message(format!("User with id={id} not found"));
    /// ```
    #[must_use]
    pub fn not_found_message(message: String) -> Self {
        Self::new(ErrorRepr::NotFound {
            message: Some(message),
        })
    }

    #[must_use]
    pub(crate) fn backtrace(&self) -> &CotBacktrace {
        &self.backtrace
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

impl From<ErrorRepr> for Error {
    fn from(value: ErrorRepr) -> Self {
        Self::new(value)
    }
}

macro_rules! impl_error_from_repr {
    ($ty:ty) => {
        impl From<$ty> for Error {
            fn from(value: $ty) -> Self {
                Error::from(ErrorRepr::from(value))
            }
        }
    };
}

impl From<Error> for rinja::Error {
    fn from(value: Error) -> Self {
        rinja::Error::Custom(Box::new(value))
    }
}

impl_error_from_repr!(toml::de::Error);
impl_error_from_repr!(rinja::Error);
impl_error_from_repr!(crate::router::path::ReverseError);
#[cfg(feature = "db")]
impl_error_from_repr!(crate::db::DatabaseError);
impl_error_from_repr!(crate::form::FormError);
impl_error_from_repr!(crate::auth::AuthError);
#[cfg(feature = "json")]
impl_error_from_repr!(serde_json::Error);
impl_error_from_repr!(crate::request::PathParamsDeserializerError);

#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum ErrorRepr {
    /// A custom user error occurred.
    #[error("{0}")]
    Custom(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// An error occurred while trying to load the config.
    #[error("Could not read the config file at `{config}` or `config/{config}.toml`")]
    LoadConfig {
        config: String,
        source: std::io::Error,
    },
    /// An error occurred while trying to parse the config.
    #[error("Could not parse the config: {source}")]
    ParseConfig {
        #[from]
        source: toml::de::Error,
    },
    /// An error occurred while trying to start the server.
    #[error("Could not start server: {source}")]
    StartServer { source: std::io::Error },
    /// An error occurred while trying to collect static files into a directory.
    #[error("Could not collect static files: {source}")]
    CollectStatic { source: std::io::Error },
    /// An error occurred while trying to read the request body.
    #[error("Could not retrieve request body: {source}")]
    ReadRequestBody {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The request body had an invalid `Content-Type` header.
    #[error("Invalid content type; expected `{expected}`, found `{actual}`")]
    InvalidContentType {
        expected: &'static str,
        actual: String,
    },
    /// Could not find a route for the request.
    #[error("Not found: {message:?}")]
    NotFound { message: Option<String> },
    /// Could not create a response object.
    #[error("Could not create a response object: {0}")]
    ResponseBuilder(#[from] http::Error),
    /// `reverse` was called on a route that does not exist.
    #[error("Failed to reverse route `{view_name}` due to view not existing")]
    NoViewToReverse { view_name: String },
    /// An error occurred while trying to reverse a route (e.g. due to missing
    /// parameters).
    #[error("Failed to reverse route: {0}")]
    ReverseRoute(#[from] crate::router::path::ReverseError),
    /// An error occurred while trying to render a template.
    #[error("Failed to render template: {0}")]
    TemplateRender(#[from] rinja::Error),
    /// An error occurred while communicating with the database.
    #[error("Database error: {0}")]
    #[cfg(feature = "db")]
    Database(#[from] crate::db::DatabaseError),
    /// An error occurred while parsing a form.
    #[error("Failed to process a form: {0}")]
    Form(#[from] crate::form::FormError),
    /// An error occurred while trying to authenticate a user.
    #[error("Failed to authenticate user: {0}")]
    Authentication(#[from] crate::auth::AuthError),
    /// An error occurred while trying to serialize or deserialize JSON.
    #[error("JSON error: {0}")]
    #[cfg(feature = "json")]
    Json(#[from] serde_json::Error),
    /// An error occurred inside a middleware-wrapped view.
    #[error("{source}")]
    MiddlewareWrapped {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// An error occurred while trying to parse path parameters.
    #[error("Could not parse path parameters: {0}")]
    PathParametersParse(#[from] crate::request::PathParamsDeserializerError),
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn test_error_new() {
        let inner = ErrorRepr::StartServer {
            source: io::Error::new(io::ErrorKind::Other, "server error"),
        };

        let error = Error::new(inner);

        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn test_error_display() {
        let inner = ErrorRepr::InvalidContentType {
            expected: "application/json",
            actual: "text/html".to_string(),
        };
        let error = Error::new(inner);

        let display = format!("{error}");

        assert_eq!(
            display,
            "Invalid content type; expected `application/json`, found `text/html`"
        );
    }

    #[test]
    fn test_error_from_repr() {
        let inner = ErrorRepr::NoViewToReverse {
            view_name: "home".to_string(),
        };

        let error: Error = inner.into();

        assert_eq!(
            format!("{error}"),
            "Failed to reverse route `home` due to view not existing"
        );
    }
}
