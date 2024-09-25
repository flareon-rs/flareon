use std::sync::Arc;

use askama::Template;
use log::error;

use crate::router::Router;
use crate::{Error, Result, StatusCode};

#[derive(Debug)]
pub(super) struct FlareonDiagnostics {
    router: Arc<Router>,
}

impl FlareonDiagnostics {
    #[must_use]
    pub(super) fn new(router: Arc<Router>) -> Self {
        Self { router }
    }
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
struct ErrorPageTemplate {
    error_data: Vec<ErrorData>,
    route_data: Vec<RouteData>,
}

#[derive(Debug)]
struct ErrorData {
    description: String,
    debug_str: String,
    is_flareon_error: bool,
}

#[derive(Debug)]
struct RouteData {
    index: String,
    path: String,
    kind: String,
    name: String,
}

#[must_use]
pub(super) fn handle_response_error(
    error: Error,
    diagnostics: FlareonDiagnostics,
) -> axum::response::Response {
    let response = build_error_response(error, diagnostics);

    match response {
        Ok(error_str) => axum::response::Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(axum::body::Body::new(error_str))
            .unwrap_or_else(|_| build_flareon_failure_page()),
        Err(error) => {
            error!("Failed to render error page: {}", error);
            build_flareon_failure_page()
        }
    }
}

fn build_error_response(error: Error, diagnostics: FlareonDiagnostics) -> Result<String> {
    let mut error_data = Vec::new();
    build_error_data(&mut error_data, &error);
    let mut route_data = Vec::new();
    build_route_data(&mut route_data, &diagnostics.router, "", "");

    let template = ErrorPageTemplate {
        error_data,
        route_data,
    };
    Ok(template.render()?)
}

fn build_route_data(
    route_data: &mut Vec<RouteData>,
    router: &Router,
    url_prefix: &str,
    index_prefix: &str,
) {
    for (index, route) in router.routes().iter().enumerate() {
        route_data.push(RouteData {
            index: format!("{index_prefix}{index}"),
            path: route.url(),
            kind: match route.kind() {
                crate::router::RouteKind::Router => if route_data.is_empty() {
                    "RootRouter"
                } else {
                    "Router"
                }
                .to_owned(),
                crate::router::RouteKind::Handler => "View".to_owned(),
            },
            name: route.name().unwrap_or_default().to_owned(),
        });

        if let Some(inner_router) = route.router() {
            build_route_data(
                route_data,
                inner_router,
                &format!("{}{}", url_prefix, route.url()),
                &format!("{index_prefix}{index}."),
            );
        }
    }
}

fn build_error_data(vec: &mut Vec<ErrorData>, error: &(dyn std::error::Error + 'static)) {
    let data = ErrorData {
        description: error.to_string(),
        debug_str: format!("{error:#?}"),
        is_flareon_error: error.is::<Error>(),
    };
    vec.push(data);

    if let Some(source) = error.source() {
        build_error_data(vec, source);
    }
}

const FAILURE_PAGE: &[u8] = include_bytes!("../templates/fail.html");

/// A last-resort error page.
///
/// This page is displayed when an error occurs that prevents Flareon from
/// rendering a proper error page. This page is very simple and should only be
/// displayed in the event of a catastrophic failure, likely caused by a bug in
/// Flareon itself.
#[must_use]
fn build_flareon_failure_page() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(axum::body::Body::from(FAILURE_PAGE))
        .expect("Building the Flareon failure page should not fail")
}
