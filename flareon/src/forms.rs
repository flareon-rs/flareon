use std::borrow::Cow;

use async_trait::async_trait;
pub use flareon_macros::Form;
use thiserror::Error;

use crate::request::Request;

/// Error occurred while processing a form.
#[derive(Debug, Error)]
pub enum FormError<T: Form> {
    /// An error occurred while processing the request, before validating the
    /// form data.
    #[error("Request error: {error}")]
    RequestError {
        #[from]
        error: crate::Error,
    },
    /// The form failed to validate.
    #[error("The form failed to validate")]
    ValidationError { context: T::Context },
}

const FORM_FIELD_REQUIRED: &str = "This field is required.";

/// An error that can occur when validating a form field.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct FormFieldValidationError {
    message: Cow<'static, str>,
}

#[derive(Debug)]
pub enum FormErrorTarget<'a> {
    Field(&'a str),
    Form,
}

impl FormFieldValidationError {
    /// Creates a new `FormFieldValidationError` from a `String`.
    #[must_use]
    pub const fn from_string(message: String) -> Self {
        Self {
            message: Cow::Owned(message),
        }
    }

    /// Creates a new `FormFieldValidationError` from a static string.
    #[must_use]
    pub const fn from_static(message: &'static str) -> Self {
        Self {
            message: Cow::Borrowed(message),
        }
    }
}

/// A trait for types that can be used as forms.
///
/// This trait is used to define a type that can be used as a form. It provides
/// a way to create a form from a request, build a context from the request, and
/// validate the form.
///
/// # Deriving
///
/// This trait can, and should be derived using the [`Form`](derive@Form) derive
/// macro. This macro generates the implementation of the trait for the type,
/// including the implementation of the [`FormContext`] trait for the context
/// type.
///
/// ```rust
/// use flareon::forms::Form;
///
/// #[derive(Form)]
/// struct MyForm {
///    #[form(opt(max_length = 100))]
///    name: String,
/// }
/// ```
#[async_trait]
pub trait Form: Sized {
    /// The context type associated with the form.
    type Context: FormContext;

    /// Creates a form from a request.
    async fn from_request(request: &mut Request) -> Result<Self, FormError<Self>>;

    /// Builds the context for the form from a request.
    async fn build_context(request: &mut Request) -> Result<Self::Context, FormError<Self>> {
        let form_data = request
            .form_data()
            .await
            .map_err(|error| FormError::RequestError { error })?;

        let mut context = Self::Context::new();
        let mut has_errors = false;

        for (field_id, value) in Request::query_pairs(&form_data) {
            let field_id = field_id.as_ref();

            if let Err(err) = context.set_value(field_id, value) {
                context.add_error(FormErrorTarget::Field(field_id), err);
                has_errors = true;
            }
        }

        if has_errors {
            Err(FormError::ValidationError { context })
        } else {
            Ok(context)
        }
    }
}

/// A trait for form contexts.
///
/// A form context is used to store the state of a form, such as the values of
/// the fields and any errors that occur during validation. This trait is used
/// to define the interface for a form context, which is used to interact with
/// the form fields and errors.
///
/// This trait is typically not implemented directly; instead, its
/// implementations are generated automatically through the
/// [`Form`](derive@Form) derive macro.
pub trait FormContext: Sized {
    /// Creates a new form context without any initial form data.
    fn new() -> Self;

    /// Returns an iterator over the fields in the form.
    fn fields(&self) -> impl Iterator<Item = &dyn DynFormField> + '_;

    /// Sets the value of a form field.
    fn set_value(
        &mut self,
        field_id: &str,
        value: Cow<str>,
    ) -> Result<(), FormFieldValidationError>;

    /// Adds a validation error to the form context.
    fn add_error(&mut self, target: FormErrorTarget, error: FormFieldValidationError) {
        self.get_errors_mut(target).push(error);
    }

    /// Returns the validation errors for a target in the form context.
    fn get_errors(&self, target: FormErrorTarget) -> &[FormFieldValidationError];

    /// Returns a mutable reference to the validation errors for a target in the
    /// form context.
    fn get_errors_mut(&mut self, target: FormErrorTarget) -> &mut Vec<FormFieldValidationError>;
}

/// Generic options valid for all types of form fields.
#[derive(Debug)]
pub struct FormFieldOptions {
    pub id: String,
}

/// A form field.
///
/// This trait is used to define a type of field that can be used in a form. It
/// is used to render the field in an HTML form, set the value of the field, and
/// validate it. Typically, the implementors of this trait are used indirectly
/// through the [`Form`] trait and field types that implement [`AsFormField`].
pub trait FormField: Sized {
    /// Custom options for the form field, unique for each field type.
    type CustomOptions: Default;

    /// Creates a new form field with the given options.
    fn with_options(options: FormFieldOptions, custom_options: Self::CustomOptions) -> Self;

    /// Returns the generic options for the form field.
    fn options(&self) -> &FormFieldOptions;

    /// Returns the ID of the form field.
    fn id(&self) -> &str {
        &self.options().id
    }

    /// Sets the string value of the form field.
    ///
    /// This method should convert the value to the appropriate type for the
    /// field, such as a number for a number field.
    fn set_value(&mut self, value: Cow<str>);

    /// Renders the form field as an HTML string.
    fn render(&self) -> String;
}

/// A version of [`FormField`] that can be used in a dynamic context.
///
/// This trait is used to allow a form field to be used in a dynamic context,
/// such as when using Form field iterator. It provides access to the field's
/// options, value, and rendering, among others.
///
/// This trait is implemented for all types that implement [`FormField`].
pub trait DynFormField {
    fn dyn_options(&self) -> &FormFieldOptions;

    fn dyn_id(&self) -> &str;

    fn dyn_set_value(&mut self, value: Cow<str>);

    fn dyn_render(&self) -> String;
}

impl<T: FormField> DynFormField for T {
    fn dyn_options(&self) -> &FormFieldOptions {
        FormField::options(self)
    }

    fn dyn_id(&self) -> &str {
        FormField::id(self)
    }

    fn dyn_set_value(&mut self, value: Cow<str>) {
        FormField::set_value(self, value);
    }

    fn dyn_render(&self) -> String {
        FormField::render(self)
    }
}

/// A trait for types that can be used as form fields.
///
/// This trait uses [`FormField`] to define a type that can be used as a form
/// field. It provides a way to clean the value of the field, which is used to
/// validate the field's value before converting to the final type.
pub trait AsFormField {
    type Type: FormField;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError>
    where
        Self: Sized;
}

/// A form field for a string.
#[derive(Debug)]
pub struct CharField {
    options: FormFieldOptions,
    custom_options: CharFieldOptions,
    value: Option<String>,
}

/// Custom options for a `CharField`.
#[derive(Debug, Default, Copy, Clone)]
pub struct CharFieldOptions {
    /// The maximum length of the field. Used to set the `maxlength` attribute
    /// in the HTML input element.
    pub max_length: Option<u32>,
}

impl CharFieldOptions {
    /// Sets the maximum length for the `CharField`.
    pub fn set_max_length(&mut self, max_length: u32) {
        self.max_length = Some(max_length);
    }
}

impl FormField for CharField {
    type CustomOptions = CharFieldOptions;

    fn with_options(options: FormFieldOptions, custom_options: Self::CustomOptions) -> Self {
        Self {
            options,
            custom_options,
            value: None,
        }
    }

    fn options(&self) -> &FormFieldOptions {
        &self.options
    }

    fn set_value(&mut self, value: Cow<str>) {
        self.value = Some(value.into_owned());
    }

    fn render(&self) -> String {
        let mut tag = HtmlTag::input("text");
        tag.attr("name", self.id());
        if let Some(max_length) = self.custom_options.max_length {
            tag.attr("maxlength", &max_length.to_string());
        }
        tag.render()
    }
}

impl AsFormField for String {
    type Type = CharField;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        if let Some(value) = &field.value {
            Ok(value.clone())
        } else {
            Err(FormFieldValidationError::from_static(FORM_FIELD_REQUIRED))
        }
    }
}

/// A helper struct for rendering HTML tags.
#[derive(Debug)]
struct HtmlTag {
    tag: String,
    attributes: Vec<(String, String)>,
}

impl HtmlTag {
    #[must_use]
    fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: Vec::new(),
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

    #[must_use]
    fn render(&self) -> String {
        let mut result = format!("<{} ", self.tag);

        for (key, value) in &self.attributes {
            result.push_str(&format!("{key}=\"{value}\" "));
        }

        result.push_str(" />");
        result
    }
}
