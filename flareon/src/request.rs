use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use indexmap::IndexMap;

use crate::headers::FORM_CONTENT_TYPE;
use crate::{Error, FlareonProject};

#[derive(Debug)]
pub struct Request {
    inner: axum::extract::Request,
    project: Arc<FlareonProject>,
    pub(crate) path_params: IndexMap<String, String>,
}

impl Request {
    #[must_use]
    pub fn new(inner: axum::extract::Request, project: Arc<FlareonProject>) -> Self {
        Self {
            inner,
            project,
            path_params: IndexMap::new(),
        }
    }

    #[must_use]
    pub fn inner(&self) -> &axum::extract::Request {
        &self.inner
    }

    #[must_use]
    pub fn project(&self) -> &FlareonProject {
        &self.project
    }

    #[must_use]
    pub fn uri(&self) -> &http::Uri {
        self.inner.uri()
    }

    #[must_use]
    pub fn method(&self) -> &http::Method {
        self.inner.method()
    }

    #[must_use]
    pub fn headers(&self) -> &http::HeaderMap {
        self.inner.headers()
    }

    #[must_use]
    pub fn content_type(&self) -> Option<&http::HeaderValue> {
        self.inner.headers().get(http::header::CONTENT_TYPE)
    }

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
    pub async fn form_data(&mut self) -> Result<Bytes, Error> {
        if self.method() == http::Method::GET || self.method() == http::Method::HEAD {
            if let Some(query) = self.inner.uri().query() {
                return Ok(Bytes::copy_from_slice(query.as_bytes()));
            }

            Ok(Bytes::new())
        } else {
            self.expect_content_type(FORM_CONTENT_TYPE)?;

            let body = std::mem::take(self.inner.body_mut());
            let bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .map_err(|err| Error::ReadRequestBody { source: err })?;

            Ok(bytes)
        }
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

    pub fn query_pairs(bytes: &Bytes) -> impl Iterator<Item = (Cow<str>, Cow<str>)> {
        form_urlencoded::parse(bytes.as_ref())
    }

    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }
}
