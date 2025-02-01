//! HTTP request type and helper methods.
//!
//! Cot uses the [`Request`](http::Request) type from the [`http`] crate
//! to represent incoming HTTP requests. However, it also provides a
//! [`RequestExt`] trait that contain various helper methods for working with
//! HTTP requests. These methods are used to access the application context,
//! project configuration, path parameters, and more. You probably want to have
//! a `use` statement for [`RequestExt`] in your code most of the time to be
//! able to use these functions:
//!
//! ```
//! use cot::request::RequestExt;
//! ```

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use indexmap::IndexMap;
pub use path_params_deserializer::PathParamsDeserializerError;
use tower_sessions::Session;

#[cfg(feature = "db")]
use crate::db::Database;
use crate::error::ErrorRepr;
use crate::headers::FORM_CONTENT_TYPE;
#[cfg(feature = "json")]
use crate::headers::JSON_CONTENT_TYPE;
use crate::router::Router;
use crate::{Body, Result};

mod path_params_deserializer;

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
/// outside the context of Cot.
#[async_trait]
pub trait RequestExt: private::Sealed {
    /// Get the application context.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let context = request.context();
    ///     // ... do something with the context
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn context(&self) -> &crate::AppContext;

    /// Get the project configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let config = request.project_config();
    ///     // ... do something with the config
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn project_config(&self) -> &crate::config::ProjectConfig;

    /// Get the router.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let router = request.router();
    ///     // ... do something with the router
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn router(&self) -> &Router;

    /// Get the route name, or [`None`] if the request is not routed or doesn't
    /// have a route name.
    ///
    /// This is mainly useful for use in templates, where you want to know which
    /// route is being rendered, for instance to mark the active tab.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let route_name = request.route_name();
    ///     // ... do something with the route name
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn route_name(&self) -> Option<&str>;

    /// Get the path parameters.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let path_params = request.path_params();
    ///     // ... do something with the path params
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn path_params(&self) -> &PathParams;

    /// Get the path parameters mutably.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let path_params = request.path_params_mut();
    ///     // ... do something with the path params
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn path_params_mut(&mut self) -> &mut PathParams;

    /// Get the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let db = request.db();
    ///     // ... do something with the database
    ///     # todo!()
    /// }
    /// ```
    #[cfg(feature = "db")]
    #[must_use]
    fn db(&self) -> &Database;

    /// Get the session object.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn hello(request: Request) -> cot::Result<Response> {
    ///     let name: String = request
    ///         .session()
    ///         .get("user_name")
    ///         .await
    ///         .expect("Invalid session value")
    ///         .unwrap_or_default();
    ///     println!("Hello, {}!", name);
    ///
    ///     // ...
    ///     # todo!()
    /// }
    ///
    /// async fn set_name(mut request: Request) -> cot::Result<Response> {
    ///     request
    ///         .session_mut()
    ///         .insert("user_name", "test_user")
    ///         .await
    ///         .unwrap();
    ///
    ///     // ...
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn session(&self) -> &Session;

    /// Get the session object mutably.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn hello(request: Request) -> cot::Result<Response> {
    ///     let name: String = request
    ///         .session()
    ///         .get("user_name")
    ///         .await
    ///         .expect("Invalid session value")
    ///         .unwrap_or_default();
    ///     println!("Hello, {}!", name);
    ///
    ///     // ...
    ///     # todo!()
    /// }
    ///
    /// async fn set_name(mut request: Request) -> cot::Result<Response> {
    ///     request
    ///         .session_mut()
    ///         .insert("user_name", "test_user")
    ///         .await
    ///         .unwrap();
    ///
    ///     // ...
    ///     # todo!()
    /// }
    /// ```
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
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::{Response, ResponseExt};
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Serialize, Deserialize)]
    /// struct MyData {
    ///     hello: String,
    /// }
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let data: MyData = request.json().await?;
    ///     Ok(Response::new_json(cot::StatusCode::OK, &data)?)
    /// }
    /// ```
    #[cfg(feature = "json")]
    async fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T>;

    /// Get the content type of the request.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     let content_type = request.content_type();
    ///     // ... do something with the content type
    ///     # todo!()
    /// }
    /// ```
    #[must_use]
    fn content_type(&self) -> Option<&http::HeaderValue>;

    /// Expect the content type of the request to be the given value.
    ///
    /// # Errors
    ///
    /// Throws an error if the content type is not the expected value.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::{Request, RequestExt};
    /// use cot::response::Response;
    ///
    /// async fn my_handler(mut request: Request) -> cot::Result<Response> {
    ///     request.expect_content_type("application/json")?;
    ///     // ...
    ///     # todo!()
    /// }
    /// ```
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

    fn route_name(&self) -> Option<&str> {
        self.extensions()
            .get::<RouteName>()
            .map(|RouteName(name)| name.as_str())
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

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RouteName(pub(crate) String);

/// Path parameters extracted from the request URL, and available as a map of
/// strings.
///
/// This struct is meant to be mainly used using the [`PathParams::parse`]
/// method, which will deserialize the path parameters into a type `T`
/// implementing `serde::DeserializeOwned`. If needed, you can also access the
/// path parameters directly using the [`PathParams::get`] method.
///
/// # Examples
///
/// ```
/// use cot::request::{PathParams, Request, RequestExt};
/// use cot::response::Response;
/// ///
/// use cot::test::TestRequestBuilder;
///
/// async fn my_handler(mut request: Request) -> cot::Result<Response> {
///     let path_params = request.path_params();
///     let name = path_params.get("name").unwrap();
///
///     // using more ergonomic syntax:
///     let name: String = request.path_params().parse()?;
///
///     let name = println!("Hello, {}!", name);
///     // ...
///     # todo!()
/// }
/// ```
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
    /// Creates a new [`PathParams`] instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// assert_eq!(path_params.get("name"), Some("world"));
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            params: IndexMap::new(),
        }
    }

    /// Inserts a new path parameter.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// assert_eq!(path_params.get("name"), Some("world"));
    /// ```
    pub fn insert(&mut self, name: String, value: String) {
        self.params.insert(name, value);
    }

    /// Iterates over the path parameters.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// for (name, value) in path_params.iter() {
    ///     println!("{}: {}", name, value);
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.params
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Returns the number of path parameters.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let path_params = PathParams::new();
    /// assert_eq!(path_params.len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// Returns `true` if the path parameters are empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let path_params = PathParams::new();
    /// assert!(path_params.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    /// Returns the value of a path parameter.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// assert_eq!(path_params.get("name"), Some("world"));
    /// ```
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(String::as_str)
    }

    /// Returns the value of a path parameter at the given index.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// assert_eq!(path_params.get_index(0), Some("world"));
    /// ```
    #[must_use]
    pub fn get_index(&self, index: usize) -> Option<&str> {
        self.params
            .get_index(index)
            .map(|(_, value)| value.as_str())
    }

    /// Returns the key of a path parameter at the given index.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    ///
    /// let mut path_params = PathParams::new();
    /// path_params.insert("name".into(), "world".into());
    /// assert_eq!(path_params.key_at_index(0), Some("name"));
    /// ```
    #[must_use]
    pub fn key_at_index(&self, index: usize) -> Option<&str> {
        self.params.get_index(index).map(|(key, _)| key.as_str())
    }

    /// Deserializes the path parameters into a type `T` implementing
    /// `serde::DeserializeOwned`.
    ///
    /// # Errors
    ///
    /// Throws an error if the path parameters could not be deserialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::request::PathParams;
    /// use serde::Deserialize;
    ///
    /// # fn main() -> Result<(), cot::Error> {
    /// let mut path_params = PathParams::new();
    /// path_params.insert("hello".into(), "world".into());
    /// path_params.insert("name".into(), "john".into());
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     hello: String,
    ///     name: String,
    /// }
    ///
    /// let params: Params = path_params.parse()?;
    /// assert_eq!(params.hello, "world");
    /// assert_eq!(params.name, "john");
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse<'de, T: serde::Deserialize<'de>>(
        &'de self,
    ) -> std::result::Result<T, PathParamsDeserializerError> {
        T::deserialize(path_params_deserializer::PathParamsDeserializer::new(self))
    }
}

pub(crate) fn query_pairs(bytes: &Bytes) -> impl Iterator<Item = (Cow<'_, str>, Cow<'_, str>)> {
    form_urlencoded::parse(bytes.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn form_data() {
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
    async fn json() {
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .header(http::header::CONTENT_TYPE, JSON_CONTENT_TYPE)
            .body(Body::fixed(r#"{"hello":"world"}"#))
            .unwrap();

        let data: serde_json::Value = request.json().await.unwrap();
        assert_eq!(data, serde_json::json!({"hello": "world"}));
    }

    #[test]
    fn path_params() {
        let mut path_params = PathParams::new();
        path_params.insert("name".into(), "world".into());

        assert_eq!(path_params.get("name"), Some("world"));
        assert_eq!(path_params.get("missing"), None);
    }

    #[test]
    fn path_params_parse() {
        let mut path_params = PathParams::new();
        path_params.insert("hello".into(), "world".into());
        path_params.insert("foo".into(), "bar".into());

        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Params {
            hello: String,
            foo: String,
        }

        let params: Params = path_params.parse().unwrap();
        assert_eq!(
            params,
            Params {
                hello: "world".to_string(),
                foo: "bar".to_string(),
            }
        );
    }

    #[test]
    fn create_query_pairs() {
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
