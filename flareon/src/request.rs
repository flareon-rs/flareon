use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::BodyExt;
use indexmap::IndexMap;

use crate::headers::FORM_CONTENT_TYPE;
use crate::{Body, Error, FlareonProject};

pub type Request = http::Request<Body>;

#[async_trait]
pub trait RequestExt {
    #[must_use]
    fn project(&self) -> &FlareonProject;

    #[must_use]
    fn path_params(&self) -> &PathParams;

    #[must_use]
    fn path_params_mut(&mut self) -> &mut PathParams;

    /// Get the request body as bytes. If the request method is GET or HEAD, the
    /// query string is returned. Otherwise, if the request content type is
    /// `application/x-www-form-urlencoded`, then the body is read and returned.
    /// Otherwise, an error is thrown.
    ///
    /// # Errors
    ///
    /// Throws an error if the request method is not GET or HEAD and the content
    /// type is not `application/x-www-form-urlencoded`.
    /// Throws an error if the request body could not be read.
    ///
    /// # Returns
    ///
    /// The request body as bytes.
    async fn form_data(&mut self) -> Result<Bytes, Error>;

    #[must_use]
    fn content_type(&self) -> Option<&http::HeaderValue>;

    fn expect_content_type(&mut self, expected: &'static str) -> Result<(), Error>;
}

#[async_trait]
impl RequestExt for Request {
    fn project(&self) -> &FlareonProject {
        self.extensions()
            .get::<Arc<FlareonProject>>()
            .expect("FlareonProject extension missing")
    }

    fn path_params(&self) -> &PathParams {
        self.extensions()
            .get::<PathParams>()
            .expect("PathParams extension missing")
    }

    fn path_params_mut(&mut self) -> &mut PathParams {
        self.extensions_mut()
            .get_mut::<PathParams>()
            .expect("PathParams extension missing")
    }

    async fn form_data(&mut self) -> Result<Bytes, Error> {
        if self.method() == http::Method::GET || self.method() == http::Method::HEAD {
            if let Some(query) = self.uri().query() {
                return Ok(Bytes::copy_from_slice(query.as_bytes()));
            }

            Ok(Bytes::new())
        } else {
            self.expect_content_type(FORM_CONTENT_TYPE)?;

            let body = std::mem::take(self.body_mut());
            let bytes = body_to_bytes(body, usize::MAX).await?;

            Ok(bytes)
        }
    }

    fn content_type(&self) -> Option<&http::HeaderValue> {
        self.headers().get(http::header::CONTENT_TYPE)
    }

    fn expect_content_type(&mut self, expected: &'static str) -> Result<(), Error> {
        let content_type = self
            .content_type()
            .map_or("".into(), |value| String::from_utf8_lossy(value.as_bytes()));
        if self.content_type() == Some(&http::HeaderValue::from_static(expected)) {
            Ok(())
        } else {
            Err(Error::InvalidContentType {
                expected,
                actual: content_type.into_owned(),
            })
        }
    }
}

#[derive(Debug)]
pub struct PathParams {
    params: IndexMap<String, String>,
}

impl Default for PathParams {
    fn default() -> Self {
        Self::new()
    }
}

impl PathParams {
    #[must_use]
    pub fn new() -> Self {
        Self {
            params: IndexMap::new(),
        }
    }

    pub fn insert(&mut self, name: String, value: String) {
        self.params.insert(name, value);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(String::as_str)
    }
}

pub(crate) fn query_pairs(bytes: &Bytes) -> impl Iterator<Item = (Cow<str>, Cow<str>)> {
    form_urlencoded::parse(bytes.as_ref())
}

async fn body_to_bytes(body: Body, limit: usize) -> Result<Bytes, Error> {
    http_body_util::Limited::new(body, limit)
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
        .map_err(|source| Error::ReadRequestBody { source })
}
