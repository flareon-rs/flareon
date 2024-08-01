use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use indexmap::IndexMap;

use crate::{Error, FlareonProject, FORM_CONTENT_TYPE};

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
    pub fn uri(&self) -> &axum::http::Uri {
        self.inner.uri()
    }

    #[must_use]
    pub fn method(&self) -> &axum::http::Method {
        self.inner.method()
    }

    #[must_use]
    pub fn headers(&self) -> &axum::http::HeaderMap {
        self.inner.headers()
    }

    #[must_use]
    pub fn content_type(&self) -> Option<&axum::http::HeaderValue> {
        self.inner.headers().get(axum::http::header::CONTENT_TYPE)
    }

    pub async fn form_data(&mut self) -> Result<Bytes, Error> {
        if self.method() == axum::http::Method::GET {
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
        if self.content_type() == Some(&axum::http::HeaderValue::from_static(expected)) {
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
        self.path_params.get(name).map(std::string::String::as_str)
    }
}
