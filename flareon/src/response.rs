//! HTTP response type and helper methods.
//!
//! Flareon uses the [`Response`](http::Response) type from the [`http`] crate
//! to represent outgoing HTTP responses. However, it also provides a
//! [`ResponseExt`] trait that contain various helper methods for working with
//! HTTP responses. These methods are used to create new responses with HTML
//! content types, redirects, and more. You probably want to have a `use`
//! statement for [`ResponseExt`] in your code most of the time to be able to
//! use these functions:
//!
//! ```
//! use flareon::response::ResponseExt;
//! ```

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
/// outside the context of Flareon.
pub trait ResponseExt: Sized + private::Sealed {
    #[must_use]
    fn builder() -> http::response::Builder;

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
    /// use flareon::response::{Response, ResponseExt};
    /// use flareon::{Body, StatusCode};
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
    /// # Ok::<(), flareon::Error>(())
    /// ```
    #[cfg(feature = "json")]
    fn new_json<T: ?Sized + serde::Serialize>(status: StatusCode, data: &T) -> crate::Result<Self>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::HTML_CONTENT_TYPE;
    use crate::response::{Response, ResponseExt};
    use crate::BodyInner;

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
