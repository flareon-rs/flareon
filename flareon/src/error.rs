use thiserror::Error;

#[derive(Debug, Error)]
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
}
