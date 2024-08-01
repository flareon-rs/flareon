use log::debug;

#[derive(Debug, Clone)]
pub(super) struct PathMatcher {
    /// The path pattern to match against; kept here mostly for debugging
    /// purposes (since a compiled regex is used internally)
    path_pattern: String,
    param_names: Vec<String>,
    regex: regex::Regex,
}

impl PathMatcher {
    #[must_use]
    pub fn new<T: Into<String>>(path_pattern: T) -> Self {
        let path_pattern = path_pattern.into();
        let mut param_names = Vec::new();
        let regex_str = path_pattern
            .split('/')
            .map(|part| {
                if part.starts_with(':') {
                    param_names.push(part[1..].to_string());
                    "([^/]+)"
                } else {
                    part
                }
            })
            .collect::<Vec<&str>>()
            .join("/");
        let regex_str = format!("^{}", regex_str);
        debug!(
            "Generated regex: `{}` for path `{}`",
            regex_str, path_pattern
        );
        let regex = regex::Regex::new(&regex_str).unwrap();
        Self {
            path_pattern,
            param_names,
            regex,
        }
    }

    #[must_use]
    pub fn capture<'matcher, 'path>(
        &'matcher self,
        path: &'path str,
    ) -> Option<CaptureResult<'matcher, 'path>> {
        debug!(
            "Matching path `{}` against pattern `{}`",
            path, self.path_pattern
        );
        let captures = self.regex.captures(path)?;
        let mut params = Vec::with_capacity(self.param_names.len());
        for (i, name) in self.param_names.iter().enumerate() {
            params.push(PathParam::new(name, &captures[i + 1]));
        }
        let remaining_path = &path[captures.get(0).unwrap().end()..];
        Some(CaptureResult::new(params, remaining_path))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CaptureResult<'matcher, 'path> {
    params: Vec<PathParam<'matcher>>,
    remaining_path: &'path str,
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
    pub fn matches_fully(&self) -> bool {
        self.remaining_path.is_empty()
    }

    #[must_use]
    pub fn remaining_path(&self) -> &'path str {
        self.remaining_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathParam<'a> {
    name: &'a str,
    value: String,
}

impl<'a> PathParam<'a> {
    #[must_use]
    pub fn new(name: &'a str, value: &str) -> Self {
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
    fn test_path_parser_no_params() {
        let path_parser = PathMatcher::new("/users");
        assert_eq!(
            path_parser.capture("/users"),
            Some(CaptureResult::new(vec![], ""))
        );
        assert_eq!(path_parser.capture("/test"), None);
    }

    #[test]
    fn test_path_parser_single_param() {
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
    fn test_path_parser_multiple_params() {
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
}
