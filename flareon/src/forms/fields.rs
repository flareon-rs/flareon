use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::num::{
    NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroIsize, NonZeroU128,
    NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize,
};

use derive_more::Deref;

use crate::forms::{AsFormField, FormField, FormFieldOptions, FormFieldValidationError};
use crate::{Html, Render};

macro_rules! impl_form_field {
    ($field_type_name:ident, $field_options_type_name:ident, $purpose:literal $(, $generic_param:ident $(: $generic_param_bound:ident $(+ $generic_param_bound_more:ident)*)?)?) => {
        #[derive(Debug)]
        #[doc = concat!("A form field for ", $purpose, ".")]
        pub struct $field_type_name $(<$generic_param>)? {
            options: FormFieldOptions,
            custom_options: $field_options_type_name $(<$generic_param>)?,
            value: Option<String>,
        }

        impl $(<$generic_param $(: $generic_param_bound $(+ $generic_param_bound_more)* )?>)? FormField for $field_type_name $(<$generic_param>)? {
            type CustomOptions = $field_options_type_name $(<$generic_param>)?;

            fn with_options(
                options: FormFieldOptions,
                custom_options: Self::CustomOptions,
            ) -> Self {
                Self {
                    options,
                    custom_options,
                    value: None,
                }
            }

            fn options(&self) -> &FormFieldOptions {
                &self.options
            }

            fn value(&self) -> Option<&str> {
                self.value.as_deref()
            }

            fn set_value(&mut self, value: Cow<str>) {
                self.value = Some(value.into_owned());
            }
        }
    };
}

impl_form_field!(StringField, StringFieldOptions, "a string");

/// Custom options for a [`StringField`].
#[derive(Debug, Default, Copy, Clone)]
pub struct StringFieldOptions {
    /// The maximum length of the field. Used to set the `maxlength` attribute
    /// in the HTML input element.
    pub max_length: Option<u32>,
}

impl Render for StringField {
    fn render(&self) -> Html {
        let mut tag = HtmlTag::input("text");
        tag.attr("name", self.id());
        if self.options.required {
            tag.bool_attr("required");
        }
        if let Some(max_length) = self.custom_options.max_length {
            tag.attr("maxlength", &max_length.to_string());
        }
        tag.render()
    }
}

impl AsFormField for String {
    type Type = StringField;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        let value = check_required(field)?;

        if let Some(max_length) = field.custom_options.max_length {
            if value.len() > max_length as usize {
                return Err(FormFieldValidationError::maximum_length_exceeded(
                    max_length,
                ));
            }
        }
        Ok(value.to_owned())
    }
}

/// A newtype for String holding a password.
///
/// This type is used to avoid accidentally logging or displaying passwords in
/// debug output or other places where the password should be kept secret.
///
/// This type also implements `AsFormField` to allow it to be used in forms as
/// an HTML password field.
#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref)]
pub struct Password(pub String);

impl Password {
    /// Creates a new `Password` from a string.
    #[must_use]
    pub fn new<T: Into<String>>(password: T) -> Self {
        Self(password.into())
    }

    /// Returns the password as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the `Password` and returns the inner string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Password").field(&"********").finish()
    }
}

impl Display for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "********")
    }
}

impl From<String> for Password {
    fn from(password: String) -> Self {
        Self(password)
    }
}

impl_form_field!(PasswordField, PasswordFieldOptions, "a password");

/// Custom options for a [`PasswordField`].
#[derive(Debug, Default, Copy, Clone)]
pub struct PasswordFieldOptions {
    /// The maximum length of the field. Used to set the `maxlength` attribute
    /// in the HTML input element.
    pub max_length: Option<u32>,
}

impl Render for PasswordField {
    fn render(&self) -> Html {
        let mut tag = HtmlTag::input("password");
        tag.attr("name", self.id());
        if self.options.required {
            tag.bool_attr("required");
        }
        if let Some(max_length) = self.custom_options.max_length {
            tag.attr("maxlength", &max_length.to_string());
        }
        tag.render()
    }
}

impl AsFormField for Password {
    type Type = PasswordField;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        let value = check_required(field)?;

        if let Some(max_length) = field.custom_options.max_length {
            if value.len() > max_length as usize {
                return Err(FormFieldValidationError::maximum_length_exceeded(
                    max_length,
                ));
            }
        }

        Ok(Password::new(value))
    }
}

impl_form_field!(IntegerField, IntegerFieldOptions, "an integer", T: Integer);

/// Custom options for a `IntegerField`.
#[derive(Debug, Copy, Clone)]
pub struct IntegerFieldOptions<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

impl<T: Integer> Default for IntegerFieldOptions<T> {
    fn default() -> Self {
        Self {
            min: T::MIN,
            max: T::MAX,
        }
    }
}

impl<T: Integer> Render for IntegerField<T> {
    fn render(&self) -> Html {
        let mut tag = HtmlTag::input("number");
        tag.attr("name", self.id());
        if self.options.required {
            tag.bool_attr("required");
        }
        if let Some(min) = &self.custom_options.min {
            tag.attr("min", &min.to_string());
        }
        if let Some(max) = &self.custom_options.max {
            tag.attr("max", &max.to_string());
        }
        tag.render()
    }
}

/// A trait for numerical types that optionally have minimum and maximum values.
pub trait Integer: Sized + ToString {
    const MIN: Option<Self>;
    const MAX: Option<Self>;
}

macro_rules! impl_integer {
    ($type:ty) => {
        impl Integer for $type {
            const MAX: Option<Self> = Some(Self::MAX);
            const MIN: Option<Self> = Some(Self::MIN);
        }
    };
}

impl_integer!(i8);
impl_integer!(i16);
impl_integer!(i32);
impl_integer!(i64);
impl_integer!(i128);
impl_integer!(isize);
impl_integer!(u8);
impl_integer!(u16);
impl_integer!(u32);
impl_integer!(u64);
impl_integer!(u128);
impl_integer!(usize);
impl_integer!(NonZeroI8);
impl_integer!(NonZeroI16);
impl_integer!(NonZeroI32);
impl_integer!(NonZeroI64);
impl_integer!(NonZeroI128);
impl_integer!(NonZeroIsize);
impl_integer!(NonZeroU8);
impl_integer!(NonZeroU16);
impl_integer!(NonZeroU32);
impl_integer!(NonZeroU64);
impl_integer!(NonZeroU128);
impl_integer!(NonZeroUsize);

macro_rules! impl_integer_as_form_field {
    ($type:ty) => {
        impl AsFormField for $type {
            type Type = IntegerField<$type>;

            fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
                if let Some(value) = &field.value {
                    Ok(value
                        .parse()
                        .map_err(|_| FormFieldValidationError::invalid_value(value))?)
                } else {
                    Err(FormFieldValidationError::Required)
                }
            }
        }
    };
}

impl_integer_as_form_field!(i8);
impl_integer_as_form_field!(i16);
impl_integer_as_form_field!(i32);
impl_integer_as_form_field!(i64);
impl_integer_as_form_field!(i128);
impl_integer_as_form_field!(isize);
impl_integer_as_form_field!(u8);
impl_integer_as_form_field!(u16);
impl_integer_as_form_field!(u32);
impl_integer_as_form_field!(u64);
impl_integer_as_form_field!(u128);
impl_integer_as_form_field!(usize);
impl_integer_as_form_field!(NonZeroI8);
impl_integer_as_form_field!(NonZeroI16);
impl_integer_as_form_field!(NonZeroI32);
impl_integer_as_form_field!(NonZeroI64);
impl_integer_as_form_field!(NonZeroI128);
impl_integer_as_form_field!(NonZeroIsize);
impl_integer_as_form_field!(NonZeroU8);
impl_integer_as_form_field!(NonZeroU16);
impl_integer_as_form_field!(NonZeroU32);
impl_integer_as_form_field!(NonZeroU64);
impl_integer_as_form_field!(NonZeroU128);
impl_integer_as_form_field!(NonZeroUsize);

impl_form_field!(BoolField, BoolFieldOptions, "a boolean");

/// Custom options for a `BoolField`.
#[derive(Debug, Default, Copy, Clone)]
pub struct BoolFieldOptions {
    /// If `true`, the field must be checked to be considered valid.
    pub must_be_true: Option<bool>,
}

impl Render for BoolField {
    fn render(&self) -> Html {
        let mut bool_input = HtmlTag::input("checkbox");
        bool_input.attr("name", self.id());
        bool_input.attr("value", "1");

        if self.custom_options.must_be_true.unwrap_or(false) {
            bool_input.bool_attr("required");
            return bool_input.render();
        }

        // Web browsers don't send anything when a checkbox is unchecked, so we
        // need to add a hidden input to send a "false" value.
        let mut hidden_input = HtmlTag::input("hidden");
        hidden_input.attr("name", self.id());
        hidden_input.attr("value", "0");
        let hidden = hidden_input.render();

        let checkbox = bool_input.render();
        format!("{}{}", hidden.as_str(), checkbox.as_str()).into()
    }
}

/// Implementation of `AsFormField` for `bool`.
///
/// This implementation converts the string values "true", "on", and "1" to
/// `true`, and "false", "  off", and "0" to `false`. It returns an error if the
/// value is not one of these strings. If the field is required to be `true` by
/// the field's options, it will return an error if the value is `false`.
impl AsFormField for bool {
    type Type = BoolField;

    fn new_field(
        mut options: FormFieldOptions,
        custom_options: <Self::Type as FormField>::CustomOptions,
    ) -> Self::Type {
        options.required = false;
        Self::Type::with_options(options, custom_options)
    }

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        let value = check_required(field)?;
        let value = if ["true", "on", "1"].contains(&value) {
            true
        } else if ["false", "off", "0"].contains(&value) {
            false
        } else {
            return Err(FormFieldValidationError::invalid_value(value));
        };

        if field.custom_options.must_be_true.unwrap_or(false) && !value {
            return Err(FormFieldValidationError::BooleanRequiredToBeTrue);
        }
        Ok(value.to_owned())
    }
}

impl<T: AsFormField> AsFormField for Option<T> {
    type Type = T::Type;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        let value = T::clean_value(field);
        match value {
            Ok(value) => Ok(Some(value)),
            Err(FormFieldValidationError::Required) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn check_required<T: FormField>(field: &T) -> Result<&str, FormFieldValidationError> {
    if let Some(value) = field.value() {
        Ok(value)
    } else {
        Err(FormFieldValidationError::Required)
    }
}

/// A helper struct for rendering HTML tags.
#[derive(Debug)]
struct HtmlTag {
    tag: String,
    attributes: Vec<(String, String)>,
    boolean_attributes: Vec<String>,
}

impl HtmlTag {
    #[must_use]
    fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: Vec::new(),
            boolean_attributes: Vec::new(),
        }
    }

    #[must_use]
    fn input(input_type: &str) -> Self {
        let mut input = Self::new("input");
        input.attr("type", input_type);
        input
    }

    fn attr(&mut self, key: &str, value: &str) -> &mut Self {
        assert!(
            !self.attributes.iter().any(|(k, _)| k == key),
            "Attribute already exists: {key}"
        );
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }

    fn bool_attr(&mut self, key: &str) -> &mut Self {
        self.boolean_attributes.push(key.to_string());
        self
    }

    #[must_use]
    fn render(&self) -> Html {
        let mut result = format!("<{}", self.tag);

        for (key, value) in &self.attributes {
            result.push_str(&format!(" {key}=\"{value}\""));
        }
        for key in &self.boolean_attributes {
            result.push_str(&format!(" {key}"));
        }

        result.push_str(" />");
        result.into()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn string_field_render() {
        let field = StringField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            StringFieldOptions {
                max_length: Some(10),
            },
        );
        let html = field.render().to_string();
        assert!(html.contains("type=\"text\""));
        assert!(html.contains("required"));
        assert!(html.contains("maxlength=\"10\""));
    }

    #[test]
    fn password_field_render() {
        let field = PasswordField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            PasswordFieldOptions {
                max_length: Some(10),
            },
        );
        let html = field.render().to_string();
        assert!(html.contains("type=\"password\""));
        assert!(html.contains("required"));
        assert!(html.contains("maxlength=\"10\""));
    }

    #[test]
    fn integer_field_render() {
        let field = IntegerField::<i32>::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            IntegerFieldOptions {
                min: Some(1),
                max: Some(10),
            },
        );
        let html = field.render().to_string();
        assert!(html.contains("type=\"number\""));
        assert!(html.contains("required"));
        assert!(html.contains("min=\"1\""));
        assert!(html.contains("max=\"10\""));
    }

    #[test]
    fn bool_field_render() {
        let field = BoolField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            BoolFieldOptions {
                must_be_true: Some(false),
            },
        );
        let html = field.render().to_string();
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("type=\"hidden\""));
        assert!(!html.contains("required"));
    }

    #[test]
    fn bool_field_render_must_be_true() {
        let field = BoolField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            BoolFieldOptions {
                must_be_true: Some(true),
            },
        );
        let html = field.render().to_string();
        assert!(html.contains("type=\"checkbox\""));
        assert!(!html.contains("type=\"hidden\""));
        assert!(html.contains("required"));
    }

    #[test]
    fn string_field_clean_value() {
        let mut field = StringField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            StringFieldOptions {
                max_length: Some(10),
            },
        );
        field.set_value(Cow::Borrowed("test"));
        let value = String::clean_value(&field).unwrap();
        assert_eq!(value, "test");
    }

    #[test]
    fn password_field_clean_value() {
        let mut field = PasswordField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            PasswordFieldOptions {
                max_length: Some(10),
            },
        );
        field.set_value(Cow::Borrowed("password"));
        let value = Password::clean_value(&field).unwrap();
        assert_eq!(value.as_str(), "password");
    }

    #[test]
    fn integer_field_clean_value() {
        let mut field = IntegerField::<i32>::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            IntegerFieldOptions {
                min: Some(1),
                max: Some(10),
            },
        );
        field.set_value(Cow::Borrowed("5"));
        let value = i32::clean_value(&field).unwrap();
        assert_eq!(value, 5);
    }

    #[test]
    fn bool_field_clean_value() {
        let mut field = BoolField::with_options(
            FormFieldOptions {
                id: "test".to_owned(),
                required: true,
            },
            BoolFieldOptions {
                must_be_true: Some(true),
            },
        );
        field.set_value(Cow::Borrowed("true"));
        let value = bool::clean_value(&field).unwrap();
        assert!(value);
    }
}
