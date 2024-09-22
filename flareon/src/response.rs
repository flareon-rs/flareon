use crate::headers::HTML_CONTENT_TYPE;
use crate::{Body, StatusCode};

const RESPONSE_BUILD_FAILURE: &str = "Failed to build response";

pub type Response = http::Response<Body>;

pub trait ResponseExt {
    #[must_use]
    fn new_html(status: StatusCode, body: Body) -> Self;

    #[must_use]
    fn new_redirect<T: Into<String>>(location: T) -> Self;
}

impl ResponseExt for Response {
    fn new_html(status: StatusCode, body: Body) -> Self {
        http::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, HTML_CONTENT_TYPE)
            .body(body)
            .expect(RESPONSE_BUILD_FAILURE)
    }

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
    fn test_response_new_html() {
        let body = Body::fixed("<html></html>");
        let response = Response::new_html(StatusCode::OK, body);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE).unwrap(),
            HTML_CONTENT_TYPE
        );
    }

    #[test]
    fn test_response_new_redirect() {
        let location = "http://example.com";
        let response = Response::new_redirect(location);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(http::header::LOCATION).unwrap(),
            location
        );
    }
}
