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
    inner: ErrorRepr,
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

    #[must_use]
    pub fn custom<E>(error: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::new(ErrorRepr::Custom(error.into()))
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

impl_error_from_repr!(rinja::Error);
impl_error_from_repr!(crate::router::path::ReverseError);
#[cfg(feature = "db")]
impl_error_from_repr!(crate::db::DatabaseError);
impl_error_from_repr!(crate::forms::FormError);
impl_error_from_repr!(crate::auth::AuthError);
#[cfg(feature = "json")]
impl_error_from_repr!(serde_json::Error);

#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum ErrorRepr {
    /// A custom user error occurred.
    #[error("{0}")]
    Custom(#[source] Box<dyn std::error::Error + Send + Sync>),
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
    /// Could not create a response object.
    #[error("Could not create a response object: {0}")]
    ResponseBuilder(#[from] http::Error),
    /// `reverse` was called on a route that does not exist.
    #[error("Failed to reverse route `{view_name}` due to view not existing")]
    NoViewToReverse { view_name: String },
    /// An error occurred while trying to reverse a route (e.g. due to missing
    /// parameters).
    #[error("Failed to reverse route: {0}")]
    ReverseError(#[from] crate::router::path::ReverseError),
    /// An error occurred while trying to render a template.
    #[error("Failed to render template: {0}")]
    TemplateRender(#[from] rinja::Error),
    /// An error occurred while communicating with the database.
    #[error("Database error: {0}")]
    #[cfg(feature = "db")]
    DatabaseError(#[from] crate::db::DatabaseError),
    /// An error occurred while parsing a form.
    #[error("Failed to process a form: {0}")]
    FormError(#[from] crate::forms::FormError),
    /// An error occurred while trying to authenticate a user.
    #[error("Failed to authenticate user: {0}")]
    AuthenticationError(#[from] crate::auth::AuthError),
    /// An error occurred while trying to serialize or deserialize JSON.
    #[error("JSON error: {0}")]
    #[cfg(feature = "json")]
    JsonError(#[from] serde_json::Error),
    /// An error occurred inside a middleware-wrapped view.
    #[error("{source}")]
    MiddlewareWrapped {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
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
