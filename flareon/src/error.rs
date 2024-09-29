use thiserror::Error;

/// An error that can occur while using Flareon.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct Error {
    inner: ErrorRepr,
}

impl Error {
    #[must_use]
    pub(crate) fn new(inner: ErrorRepr) -> Self {
        Self { inner }
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

impl From<Error> for askama::Error {
    fn from(value: Error) -> Self {
        askama::Error::Custom(Box::new(value))
    }
}

impl_error_from_repr!(askama::Error);
impl_error_from_repr!(crate::router::path::ReverseError);
impl_error_from_repr!(crate::db::DatabaseError);

#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum ErrorRepr {
    /// An error occurred while trying to start the server.
    #[error("Could not start server: {source}")]
    StartServer { source: std::io::Error },
    /// An error occurred while trying to read the request body.
    #[error("Could not retrieve request body: {source}")]
    ReadRequestBody {
        #[from]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The request body had an invalid `Content-Type` header.
    #[error("Invalid content type; expected {expected}, found {actual}")]
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
    TemplateRender(#[from] askama::Error),
    /// An error occurred while communicating with the database.
    #[error("Database error: {0}")]
    DatabaseError(#[from] crate::db::DatabaseError),
}
