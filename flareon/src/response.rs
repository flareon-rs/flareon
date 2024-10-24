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
pub trait ResponseExt: private::Sealed {
    #[must_use]
    fn builder() -> http::response::Builder;

    #[must_use]
    fn new_html(status: StatusCode, body: Body) -> Self;

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
