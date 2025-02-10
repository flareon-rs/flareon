//! HTTP response type and helper methods.
//!
//! Cot uses the [`Response`](http::Response) type from the [`http`] crate
//! to represent outgoing HTTP responses. However, it also provides a
//! [`ResponseExt`] trait that contain various helper methods for working with
//! HTTP responses. These methods are used to create new responses with HTML
//! content types, redirects, and more. You probably want to have a `use`
//! statement for [`ResponseExt`] in your code most of the time to be able to
//! use these functions:
//!
//! ```
//! use cot::response::ResponseExt;
//! ```

use bytes::Bytes;
use cot::error_page::ErrorPageTrigger;

use crate::headers::HTML_CONTENT_TYPE;
#[cfg(feature = "json")]
use crate::headers::JSON_CONTENT_TYPE;
use crate::{Body, StatusCode};

const RESPONSE_BUILD_FAILURE: &str = "Failed to build response";

/// HTTP response type.
pub type Response = http::Response<Body>;

mod private {
    pub trait Sealed {}
}

/// Extension trait for [`http::Response`] that provides helper methods for
/// working with HTTP response.
///
/// # Sealed
///
/// This trait is sealed since it doesn't make sense to be implemented for types
/// outside the context of Cot.
pub trait ResponseExt: Sized + private::Sealed {
    /// Create a new response builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::response::{Response, ResponseExt};
    /// use cot::StatusCode;
    ///
    /// let response = Response::builder()
    ///     .status(StatusCode::OK)
    ///     .body(cot::Body::empty())
    ///     .expect("Failed to build response");
    /// ```
    #[must_use]
    fn builder() -> http::response::Builder;

    /// Create a new HTML response.
    ///
    /// This creates a new [`Response`] object with a content type of
    /// `text/html; charset=utf-8` and given status code and body.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, StatusCode};
    ///
    /// let response = Response::new_html(StatusCode::OK, Body::fixed("Hello world!"));
    /// ```
    #[must_use]
    fn new_html(status: StatusCode, body: Body) -> Self;

    /// Create a new JSON response.
    ///
    /// This function will create a new response with a content type of
    /// `application/json` and a body that is the JSON-serialized version of the
    /// provided instance of a type implementing `serde::Serialize`.
    ///
    /// # Errors
    ///
    /// This function will return an error if the data could not be serialized
    /// to JSON.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::response::{Response, ResponseExt};
    /// use cot::{Body, StatusCode};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct MyData {
    ///     hello: String,
    /// }
    ///
    /// let data = MyData {
    ///     hello: String::from("world"),
    /// };
    /// let response = Response::new_json(StatusCode::OK, &data)?;
    /// # Ok::<(), cot::Error>(())
    /// ```
    #[cfg(feature = "json")]
    fn new_json<T: ?Sized + serde::Serialize>(status: StatusCode, data: &T) -> crate::Result<Self>;

    /// Create a new redirect response.
    ///
    /// This creates a new [`Response`] object with a status code of
    /// [`StatusCode::SEE_OTHER`] and a location header set to the provided
    /// location.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::response::{Response, ResponseExt};
    /// use cot::StatusCode;
    ///
    /// let response = Response::new_redirect("http://example.com");
    /// ```
    ///
    /// # See also
    ///
    /// * [`crate::reverse_redirect!`] – a more ergonomic way to create
    ///   redirects to internal views
    #[must_use]
    fn new_redirect<T: Into<String>>(location: T) -> Self;
}

impl private::Sealed for Response {}

impl ResponseExt for Response {
    #[must_use]
    fn builder() -> http::response::Builder {
        http::Response::builder()
    }

    #[must_use]
    fn new_html(status: StatusCode, body: Body) -> Self {
        http::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, HTML_CONTENT_TYPE)
            .body(body)
            .expect(RESPONSE_BUILD_FAILURE)
    }

    #[cfg(feature = "json")]
    fn new_json<T: ?Sized + serde::Serialize>(status: StatusCode, data: &T) -> crate::Result<Self> {
        Ok(http::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, JSON_CONTENT_TYPE)
            .body(Body::fixed(serde_json::to_string(data)?))
            .expect(RESPONSE_BUILD_FAILURE))
    }

    #[must_use]
    fn new_redirect<T: Into<String>>(location: T) -> Self {
        http::Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(http::header::LOCATION, location.into())
            .body(Body::empty())
            .expect(RESPONSE_BUILD_FAILURE)
    }
}

pub(crate) fn not_found_response(message: Option<String>) -> Response {
    let mut response = Response::new_html(
        StatusCode::NOT_FOUND,
        Body::fixed(Bytes::from("404 Not Found")),
    );
    response
        .extensions_mut()
        .insert(ErrorPageTrigger::NotFound { message });
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::BodyInner;
    use crate::headers::HTML_CONTENT_TYPE;
    use crate::response::{Response, ResponseExt};

    #[test]
    fn response_new_html() {
        let body = Body::fixed("<html></html>");
        let response = Response::new_html(StatusCode::OK, body);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE).unwrap(),
            HTML_CONTENT_TYPE
        );
    }

    #[test]
    #[cfg(feature = "json")]
    fn response_new_json() {
        #[derive(serde::Serialize)]
        struct MyData {
            hello: String,
        }

        let data = MyData {
            hello: String::from("world"),
        };
        let response = Response::new_json(StatusCode::OK, &data).unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE).unwrap(),
            JSON_CONTENT_TYPE
        );
        match &response.body().inner {
            BodyInner::Fixed(fixed) => {
                assert_eq!(fixed, r#"{"hello":"world"}"#);
            }
            _ => {
                panic!("Expected fixed body");
            }
        }
    }

    #[test]
    fn response_new_redirect() {
        let location = "http://example.com";
        let response = Response::new_redirect(location);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(http::header::LOCATION).unwrap(),
            location
        );
    }
}
