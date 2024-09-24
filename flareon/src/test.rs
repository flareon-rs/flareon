//! Test utilities for Flareon projects.

use std::future::poll_fn;

use flareon::{prepare_request, FlareonProject};
use tower::Service;

use crate::request::Request;
use crate::response::Response;
use crate::{Body, Error, Result};

/// A test client for making requests to a Flareon project.
///
/// Useful for End-to-End testing Flareon projects.
#[derive(Debug)]
pub struct Client<S> {
    project: FlareonProject<S>,
}

impl<S> Client<S>
where
    S: Service<Request, Response = Response, Error = Error> + Send + Sync + Clone + 'static,
    S::Future: Send,
{
    #[must_use]
    pub fn new(project: FlareonProject<S>) -> Self {
        Self { project }
    }

    pub async fn get(&mut self, path: &str) -> Result<Response> {
        self.request(
            http::Request::get(path)
                .body(Body::empty())
                .expect("Test request should be valid"),
        )
        .await
    }

    pub async fn request(&mut self, mut request: Request) -> Result<Response> {
        prepare_request(&mut request, self.project.router.clone());

        poll_fn(|cx| self.project.handler.poll_ready(cx)).await?;
        self.project.handler.call(request).await
    }
}
