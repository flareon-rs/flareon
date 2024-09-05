use thiserror::Error;

/// An error that can occur while using Flareon.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred while trying to start the server.
    #[error("Could not start server: {source}")]
    StartServer { source: std::io::Error },
    /// An error occurred while trying to read the request body.
    #[error("Could not retrieve request body: {source}")]
    ReadRequestBody {
        #[from]
        source: axum::Error,
    },
    /// The request body had an invalid `Content-Type` header.
    #[error("Invalid content type; expected {expected}, found {actual}")]
    InvalidContentType {
        expected: &'static str,
        actual: String,
    },
    /// Could not create a response object.
    #[error("Could not create a response object: {0}")]
    ResponseBuilder(#[from] axum::http::Error),
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

impl From<Error> for askama::Error {
    fn from(value: Error) -> Self {
        askama::Error::Custom(Box::new(value))
    }
}
