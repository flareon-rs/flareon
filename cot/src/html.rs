//! HTML rendering utilities.
//!
//! This module provides structures and methods for creating and rendering HTML
//! content.
//!
//! # Examples
//!
//! ## Creating and rendering an HTML Tag
//!
//! ```
//! use cot::html::HtmlTag;
//!
//! let tag = HtmlTag::new("br");
//! let html = tag.render();
//! assert_eq!(html.as_str(), "<br />");
//! ```
//!
//! ## Adding Attributes to an HTML Tag
//!
//! ```
//! use cot::html::HtmlTag;
//!
//! let mut tag = HtmlTag::new("input");
//! tag.attr("type", "text").attr("placeholder", "Enter text");
//! tag.bool_attr("disabled");
//! assert_eq!(
//!     tag.render().as_str(),
//!     "<input type=\"text\" placeholder=\"Enter text\" disabled />"
//! );
//! ```

use std::fmt::Write;

use derive_more::{Deref, Display, From};
use rinja::filters::Escaper;

/// A type that represents HTML content as a string.
///
/// Note that this is just a newtype wrapper around `String` and does not
/// provide any HTML escaping functionality. It is **not** guaranteed to be safe
/// from XSS attacks.
///
/// For HTML escaping, it is recommended to use the [`HtmlTag`] struct.
///
/// # Examples
///
/// ```
/// use cot::html::Html;
///
/// let html = Html::new("<div>Hello</div>");
/// assert_eq!(html.as_str(), "<div>Hello</div>");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Deref, From, Display)]
pub struct Html(String);

impl Html {
    /// Creates a new `Html` instance from a string.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::Html;
    ///
    /// let html = Html::new("<div>Hello</div>");
    /// assert_eq!(html.as_str(), "<div>Hello</div>");
    /// ```
    #[must_use]
    pub fn new<T: Into<String>>(html: T) -> Self {
        Self(html.into())
    }

    /// Returns the inner string as a `&str`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::Html;
    ///
    /// let html = Html::new("<div>Hello</div>");
    /// assert_eq!(html.as_str(), "<div>Hello</div>");
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A helper struct for rendering HTML tags.
///
/// This struct is used to build HTML tags with attributes and boolean
/// attributes. It automatically escapes all attribute values.
///
/// # Examples
///
/// ```
/// use cot::html::HtmlTag;
///
/// let mut tag = HtmlTag::new("input");
/// tag.attr("type", "text").attr("placeholder", "Enter text");
/// tag.bool_attr("disabled");
/// assert_eq!(
///     tag.render().as_str(),
///     "<input type=\"text\" placeholder=\"Enter text\" disabled />"
/// );
/// ```
#[derive(Debug)]
pub struct HtmlTag {
    tag: String,
    attributes: Vec<(String, String)>,
    boolean_attributes: Vec<String>,
}

impl HtmlTag {
    /// Creates a new `HtmlTag` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::HtmlTag;
    ///
    /// let tag = HtmlTag::new("div");
    /// assert_eq!(tag.render().as_str(), "<div />");
    /// ```
    #[must_use]
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: Vec::new(),
            boolean_attributes: Vec::new(),
        }
    }

    /// Creates a new `HtmlTag` instance for an input element.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::HtmlTag;
    ///
    /// let input = HtmlTag::input("text");
    /// assert_eq!(input.render().as_str(), "<input type=\"text\" />");
    /// ```
    #[must_use]
    pub fn input(input_type: &str) -> Self {
        let mut input = Self::new("input");
        input.attr("type", input_type);
        input
    }

    /// Adds an attribute to the HTML tag.
    ///
    /// # Safety
    ///
    /// This function will escape the attribute value. Note that it does not
    /// escape the attribute name.
    ///
    /// # Panics
    ///
    /// This function will panic if the attribute already exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::HtmlTag;
    ///
    /// let mut tag = HtmlTag::new("input");
    /// tag.attr("type", "text").attr("placeholder", "Enter text");
    /// assert_eq!(
    ///     tag.render().as_str(),
    ///     "<input type=\"text\" placeholder=\"Enter text\" />"
    /// );
    /// ```
    pub fn attr(&mut self, key: &str, value: &str) -> &mut Self {
        assert!(
            !self.attributes.iter().any(|(k, _)| k == key),
            "Attribute already exists: {key}"
        );
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }

    /// Adds a boolean attribute to the HTML tag.
    ///
    /// # Safety
    ///
    /// This function will not escape the attribute name.
    ///
    /// # Panics
    ///
    /// This function will panic if the boolean attribute already exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::HtmlTag;
    ///
    /// let mut tag = HtmlTag::new("input");
    /// tag.bool_attr("disabled");
    /// assert_eq!(tag.render().as_str(), "<input disabled />");
    /// ```
    pub fn bool_attr(&mut self, key: &str) -> &mut Self {
        assert!(
            !self.boolean_attributes.contains(&key.to_string()),
            "Boolean attribute already exists: {key}"
        );
        self.boolean_attributes.push(key.to_string());
        self
    }

    /// Renders the HTML tag.
    ///
    /// # Examples
    ///
    /// ```
    /// use cot::html::HtmlTag;
    ///
    /// let tag = HtmlTag::new("div");
    /// assert_eq!(tag.render().as_str(), "<div />");
    /// ```
    #[must_use]
    pub fn render(&self) -> Html {
        const FAIL_MSG: &str = "Failed to write HTML tag";

        let mut result = String::new();
        write!(&mut result, "<{}", self.tag).expect(FAIL_MSG);

        for (key, value) in &self.attributes {
            write!(&mut result, " {}=\"", key).expect(FAIL_MSG);
            rinja::filters::Html
                .write_escaped_str(&mut result, value)
                .expect(FAIL_MSG);
            write!(&mut result, "\"").expect(FAIL_MSG);
        }
        for key in &self.boolean_attributes {
            write!(&mut result, " {}", key).expect(FAIL_MSG);
        }

        write!(&mut result, " />").expect(FAIL_MSG);
        result.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_new() {
        let html = Html::new("<div>Hello</div>");
        assert_eq!(html.as_str(), "<div>Hello</div>");
    }

    #[test]
    fn test_html_tag_new() {
        let tag = HtmlTag::new("div");
        assert_eq!(tag.render().as_str(), "<div />");
    }

    #[test]
    fn test_html_tag_with_attributes() {
        let mut tag = HtmlTag::new("input");
        tag.attr("type", "text").attr("placeholder", "Enter text");
        assert_eq!(
            tag.render().as_str(),
            "<input type=\"text\" placeholder=\"Enter text\" />"
        );
    }

    #[test]
    fn test_html_tag_escaping() {
        let mut tag = HtmlTag::new("input");
        tag.attr("type", "text").attr("placeholder", "<>&\"'");
        assert_eq!(
            tag.render().as_str(),
            "<input type=\"text\" placeholder=\"&#60;&#62;&#38;&#34;&#39;\" />"
        );
    }

    #[test]
    fn test_html_tag_with_boolean_attributes() {
        let mut tag = HtmlTag::new("input");
        tag.bool_attr("disabled");
        assert_eq!(tag.render().as_str(), "<input disabled />");
    }

    #[test]
    fn test_html_tag_input() {
        let mut input = HtmlTag::input("text");
        input.attr("name", "username");
        assert_eq!(
            input.render().as_str(),
            "<input type=\"text\" name=\"username\" />"
        );
    }
}
