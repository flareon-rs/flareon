use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Could not retrieve request body: {source}")]
    ReadRequestBody {
        #[from]
        source: axum::Error,
    },
    #[error("Invalid content type; expected {expected}, found {actual}")]
    InvalidContentType {
        expected: &'static str,
        actual: String,
    },
    #[error("Could not create a response object: {0}")]
    ResponseBuilder(#[from] axum::http::Error),
    #[error("Failed to reverse route `{view_name}` due to view not existing")]
    NoViewToReverse { view_name: String },
    #[error("Failed to reverse route: {0}")]
    ReverseError(#[from] crate::router::path::ReverseError),
    #[error("Failed to render template: {0}")]
    TemplateRender(#[from] askama::Error),
}

impl From<Error> for askama::Error {
    fn from(value: Error) -> Self {
        askama::Error::Custom(Box::new(value))
    }
}
