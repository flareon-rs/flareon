use std::collections::HashMap;
use std::fmt::Display;

use log::debug;
use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone)]
pub(super) struct PathMatcher {
    parts: Vec<PathPart>,
}

impl PathMatcher {
    #[must_use]
    pub(crate) fn new<T: Into<String>>(path_pattern: T) -> Self {
        let path_pattern = path_pattern.into();

        let mut last_end = 0;
        let mut parts = Vec::new();
        let param_regex = Regex::new(":([^/]+)").expect("Invalid regex");
        for capture in param_regex.captures_iter(&path_pattern) {
            let full_match = capture.get(0).expect("Could not get regex match");
            let start = full_match.start();
            if start > last_end {
                parts.push(PathPart::Literal(path_pattern[last_end..start].to_string()));
            }

            let name = capture
                .get(1)
                .expect("Could not get regex capture")
                .as_str()
                .to_owned();
            assert!(
                Self::is_param_name_valid(&name),
                "Invalid parameter name: `{name}`"
            );
            parts.push(PathPart::Param { name });
            last_end = start + full_match.len();
        }
        if last_end < path_pattern.len() {
            parts.push(PathPart::Literal(path_pattern[last_end..].to_string()));
        }

        Self { parts }
    }

    fn is_param_name_valid(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let first_char = name.chars().next().expect("Empty string");
        if !first_char.is_alphabetic() && first_char != '_' {
            return false;
        }
        for ch in name.chars() {
            if !ch.is_alphanumeric() && ch != '_' {
                return false;
            }
        }
        true
    }

    #[must_use]
    pub(crate) fn capture<'matcher, 'path>(
        &'matcher self,
        path: &'path str,
    ) -> Option<CaptureResult<'matcher, 'path>> {
        debug!("Matching path `{}` against pattern `{}`", path, self);

        let mut current_path = path;
        let mut params = Vec::with_capacity(self.param_len());
        for part in &self.parts {
            match part {
                PathPart::Literal(s) => {
                    if !current_path.starts_with(s) {
                        return None;
                    }
                    current_path = &current_path[s.len()..];
                }
                PathPart::Param { name } => {
                    let next_slash = current_path.find('/');
                    let value = if let Some(next_slash) = next_slash {
                        &current_path[..next_slash]
                    } else {
                        current_path
                    };
                    if value.is_empty() {
                        return None;
                    }
                    params.push(PathParam::new(name, value));
                    current_path = &current_path[value.len()..];
                }
            }
        }

        Some(CaptureResult::new(params, current_path))
    }

    pub(crate) fn reverse(&self, params: &ReverseParamMap) -> Result<String, ReverseError> {
        let mut result = String::new();

        for part in &self.parts {
            match part {
                PathPart::Literal(s) => result.push_str(s),
                PathPart::Param { name } => {
                    let value = params
                        .get(name)
                        .ok_or_else(|| ReverseError::MissingParam(name.clone()))?;
                    result.push_str(value);
                }
            }
        }

        Ok(result)
    }

    #[must_use]
    fn param_len(&self) -> usize {
        self.parts
            .iter()
            .map(|part| match part {
                PathPart::Literal(..) => 0,
                PathPart::Param { .. } => 1,
            })
            .sum()
    }
}

impl Display for PathMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for part in &self.parts {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReverseParamMap {
    params: HashMap<String, String>,
}

impl Default for ReverseParamMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseParamMap {
    #[must_use]
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn insert<K: ToString, V: ToString>(&mut self, key: K, value: V) {
        self.params.insert(key.to_string(), value.to_string());
    }

    #[must_use]
    fn get(&self, key: &str) -> Option<&String> {
        self.params.get(key)
    }
}

#[macro_export]
macro_rules! reverse_param_map {
    ($($key:expr => $value:expr),*) => {{
        #[allow(unused_mut)]
        let mut map = $crate::router::path::ReverseParamMap::new();
        $( map.insert($key, $value); )*
        map
    }};
}

#[derive(Debug, Error)]
pub enum ReverseError {
    #[error("Missing parameter for reverse: `{0}`")]
    MissingParam(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CaptureResult<'matcher, 'path> {
    pub(super) params: Vec<PathParam<'matcher>>,
    pub(super) remaining_path: &'path str,
}

impl<'matcher, 'path> CaptureResult<'matcher, 'path> {
    #[must_use]
    fn new(params: Vec<PathParam<'matcher>>, remaining_path: &'path str) -> Self {
        Self {
            params,
            remaining_path,
        }
    }

    #[must_use]
    pub(crate) fn matches_fully(&self) -> bool {
        self.remaining_path.is_empty()
    }
}

#[derive(Debug, Clone)]
enum PathPart {
    Literal(String),
    Param { name: String },
}

impl Display for PathPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathPart::Literal(s) => write!(f, "{s}"),
            PathPart::Param { name } => write!(f, ":{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PathParam<'a> {
    pub(super) name: &'a str,
    pub(super) value: String,
}

impl<'a> PathParam<'a> {
    #[must_use]
    pub(crate) fn new(name: &'a str, value: &str) -> Self {
        Self {
            name,
            value: value.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_parser_no_params() {
        let path_parser = PathMatcher::new("/users");
        assert_eq!(
            path_parser.capture("/users"),
            Some(CaptureResult::new(vec![], ""))
        );
        assert_eq!(path_parser.capture("/test"), None);
    }

    #[test]
    fn path_parser_single_param() {
        let path_parser = PathMatcher::new("/users/:id");
        assert_eq!(
            path_parser.capture("/users/123"),
            Some(CaptureResult::new(vec![PathParam::new("id", "123")], ""))
        );
        assert_eq!(
            path_parser.capture("/users/123/"),
            Some(CaptureResult::new(vec![PathParam::new("id", "123")], "/"))
        );
        assert_eq!(
            path_parser.capture("/users/123/abc"),
            Some(CaptureResult::new(
                vec![PathParam::new("id", "123")],
                "/abc"
            ))
        );
        assert_eq!(path_parser.capture("/users/"), None);
    }

    #[test]
    fn path_parser_multiple_params() {
        let path_parser = PathMatcher::new("/users/:id/posts/:post_id");
        assert_eq!(
            path_parser.capture("/users/123/posts/456"),
            Some(CaptureResult::new(
                vec![
                    PathParam::new("id", "123"),
                    PathParam::new("post_id", "456"),
                ],
                ""
            ))
        );
        assert_eq!(
            path_parser.capture("/users/123/posts/456/abc"),
            Some(CaptureResult::new(
                vec![
                    PathParam::new("id", "123"),
                    PathParam::new("post_id", "456"),
                ],
                "/abc"
            ))
        );
    }

    #[test]
    fn reverse_with_valid_params() {
        let path_parser = PathMatcher::new("/users/:id/posts/:post_id");
        let mut params = ReverseParamMap::new();
        params.insert("id", "123");
        params.insert("post_id", "456");
        assert_eq!(
            path_parser.reverse(&params).unwrap(),
            "/users/123/posts/456"
        );
    }

    #[test]
    fn reverse_with_missing_param() {
        let path_parser = PathMatcher::new("/users/:id/posts/:post_id");
        let mut params = ReverseParamMap::new();
        params.insert("id", "123");
        let result = path_parser.reverse(&params);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing parameter for reverse: `post_id`"
        );
    }

    #[test]
    fn reverse_with_extra_param() {
        let path_parser = PathMatcher::new("/users/:id/posts/:post_id");
        let mut params = ReverseParamMap::new();
        params.insert("id", "123");
        params.insert("post_id", "456");
        params.insert("extra", "789");
        assert_eq!(
            path_parser.reverse(&params).unwrap(),
            "/users/123/posts/456"
        );
    }
}
