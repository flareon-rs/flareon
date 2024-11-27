//! HTTP request type and helper methods.
//!
//! Flareon uses the [`Request`](http::Request) type from the [`http`] crate
//! to represent incoming HTTP requests. However, it also provides a
//! [`RequestExt`] trait that contain various helper methods for working with
//! HTTP requests. These methods are used to access the application context,
//! project configuration, path parameters, and more. You probably want to have
//! a `use` statement for [`RequestExt`] in your code most of the time to be
//! able to use these functions:
//!
//! ```
//! use flareon::request::RequestExt;
//! ```

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
#[cfg(feature = "json")]
use flareon::headers::JSON_CONTENT_TYPE;
use indexmap::IndexMap;
use tower_sessions::Session;

#[cfg(feature = "db")]
use crate::db::Database;
use crate::error::ErrorRepr;
use crate::headers::FORM_CONTENT_TYPE;
use crate::router::Router;
use crate::{Body, Result};

/// HTTP request type.
pub type Request = http::Request<Body>;

mod private {
    pub trait Sealed {}
}

/// Extension trait for [`http::Request`] that provides helper methods for
/// working with HTTP requests.
///
/// # Sealed
///
/// This trait is sealed since it doesn't make sense to be implemented for types
/// outside the context of Flareon.
#[async_trait]
pub trait RequestExt: private::Sealed {
    #[must_use]
    fn context(&self) -> &crate::AppContext;

    #[must_use]
    fn project_config(&self) -> &crate::config::ProjectConfig;

    #[must_use]
    fn router(&self) -> &Router;

    #[must_use]
    fn path_params(&self) -> &PathParams;

    #[must_use]
    fn path_params_mut(&mut self) -> &mut PathParams;

    #[cfg(feature = "db")]
    #[must_use]
    fn db(&self) -> &Database;

    #[must_use]
    fn session(&self) -> &Session;

    #[must_use]
    fn session_mut(&mut self) -> &mut Session;

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
    async fn form_data(&mut self) -> Result<Bytes>;

    /// Get the request body as JSON and deserialize it into a type `T`
    /// implementing `serde::de::DeserializeOwned`.
    ///
    /// The content type of the request must be `application/json`.
    ///
    /// # Errors
    ///
    /// Throws an error if the content type is not `application/json`.
    /// Throws an error if the request body could not be read.
    /// Throws an error if the request body could not be deserialized - either
    /// because the JSON is invalid or because the deserialization to the target
    /// structure failed.
    ///
    /// # Example
    ///
    /// ```
    /// use flareon::request::{Request, RequestExt};
    /// use flareon::response::{Response, ResponseExt};
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Serialize, Deserialize)]
    /// struct MyData {
    ///     hello: String,
    /// }
    ///
    /// async fn my_handler(mut request: Request) -> flareon::Result<Response> {
    ///     let data: MyData = request.json().await?;
    ///     Ok(Response::new_json(flareon::StatusCode::OK, &data)?)
    /// }
    /// ```
    #[cfg(feature = "json")]
    async fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T>;

    #[must_use]
    fn content_type(&self) -> Option<&http::HeaderValue>;

    fn expect_content_type(&mut self, expected: &'static str) -> Result<()>;
}

impl private::Sealed for Request {}

#[async_trait]
impl RequestExt for Request {
    fn context(&self) -> &crate::AppContext {
        self.extensions()
            .get::<Arc<crate::AppContext>>()
            .expect("AppContext extension missing")
    }

    fn project_config(&self) -> &crate::config::ProjectConfig {
        self.context().config()
    }

    fn router(&self) -> &Router {
        self.context().router()
    }

    fn path_params(&self) -> &PathParams {
        self.extensions()
            .get::<PathParams>()
            .expect("PathParams extension missing")
    }

    fn path_params_mut(&mut self) -> &mut PathParams {
        self.extensions_mut().get_or_insert_default::<PathParams>()
    }

    #[cfg(feature = "db")]
    fn db(&self) -> &Database {
        self.context().database()
    }

    fn session(&self) -> &Session {
        self.extensions()
            .get::<Session>()
            .expect("Session extension missing. Did you forget to add the SessionMiddleware?")
    }

    fn session_mut(&mut self) -> &mut Session {
        self.extensions_mut()
            .get_mut::<Session>()
            .expect("Session extension missing. Did you forget to add the SessionMiddleware?")
    }

    async fn form_data(&mut self) -> Result<Bytes> {
        if self.method() == http::Method::GET || self.method() == http::Method::HEAD {
            if let Some(query) = self.uri().query() {
                return Ok(Bytes::copy_from_slice(query.as_bytes()));
            }

            Ok(Bytes::new())
        } else {
            self.expect_content_type(FORM_CONTENT_TYPE)?;

            let body = std::mem::take(self.body_mut());
            let bytes = body.into_bytes().await?;

            Ok(bytes)
        }
    }

    #[cfg(feature = "json")]
    async fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        self.expect_content_type(JSON_CONTENT_TYPE)?;

        let body = std::mem::take(self.body_mut());
        let bytes = body.into_bytes().await?;

        Ok(serde_json::from_slice(&bytes)?)
    }

    fn content_type(&self) -> Option<&http::HeaderValue> {
        self.headers().get(http::header::CONTENT_TYPE)
    }

    fn expect_content_type(&mut self, expected: &'static str) -> Result<()> {
        let content_type = self
            .content_type()
            .map_or("".into(), |value| String::from_utf8_lossy(value.as_bytes()));
        if content_type == expected {
            Ok(())
        } else {
            Err(ErrorRepr::InvalidContentType {
                expected,
                actual: content_type.into_owned(),
            }
            .into())
        }
    }
}

#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_form_data() {
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .header(http::header::CONTENT_TYPE, FORM_CONTENT_TYPE)
            .body(Body::fixed("hello=world"))
            .unwrap();

        let bytes = request.form_data().await.unwrap();
        assert_eq!(bytes, Bytes::from_static(b"hello=world"));
    }

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn test_json() {
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .header(http::header::CONTENT_TYPE, JSON_CONTENT_TYPE)
            .body(Body::fixed(r#"{"hello":"world"}"#))
            .unwrap();

        let data: serde_json::Value = request.json().await.unwrap();
        assert_eq!(data, serde_json::json!({"hello": "world"}));
    }

    #[test]
    fn test_path_params() {
        let mut path_params = PathParams::new();
        path_params.insert("name".into(), "world".into());

        assert_eq!(path_params.get("name"), Some("world"));
        assert_eq!(path_params.get("missing"), None);
    }

    #[test]
    fn test_query_pairs() {
        let bytes = Bytes::from_static(b"hello=world&foo=bar");
        let pairs: Vec<_> = query_pairs(&bytes).collect();
        assert_eq!(
            pairs,
            vec![
                (Cow::from("hello"), Cow::from("world")),
                (Cow::from("foo"), Cow::from("bar"))
            ]
        );
    }
}
