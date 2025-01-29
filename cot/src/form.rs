//! Forms and form fields for handling user input.
//!
//! This module provides a way to define forms and form fields for handling user
//! input in a web application. It provides a way to create forms from requests,
//! validate the form data, and render the form fields in an HTML form.
//!
//! # `Form` derive macro
//!
//! The easiest way to work with forms in Cot is to use the
//! [`Form`](derive@Form) derive macro. Just define a structure that will hold
//! all the form data you need, and derive the [`Form`] trait for it.
//!
//! ```
//! use cot::form::Form;
//!
//! #[derive(Form)]
//! struct MyForm {
//!     #[form(opt(max_length = 100))]
//!     name: String,
//! }
//! ```

/// Built-in form fields that can be used in a form.
pub mod fields;

use std::borrow::Cow;
use std::fmt::Debug;

use async_trait::async_trait;
pub use cot_macros::Form;
use thiserror::Error;

use crate::request::{Request, RequestExt};
use crate::{request, Html, Render};

/// Error occurred while processing a form.
#[derive(Debug, Error)]
pub enum FormError {
    /// An error occurred while processing the request, before validating the
    /// form data.
    #[error("Request error: {error}")]
    RequestError {
        /// The error that occurred while processing the request.
        #[from]
        error: Box<crate::Error>,
    },
}

/// The result of validating a form.
///
/// This enum is used to represent the result of validating a form. In the case
/// of a successful validation, the `Ok` variant contains the form object. In
/// the case of a failed validation, the `ValidationError` variant contains the
/// context object with the validation errors, as well as the user's input.
#[must_use]
#[derive(Debug, Clone)]
pub enum FormResult<T: Form> {
    /// The form validation passed.
    Ok(T),
    /// The form validation failed.
    ValidationError(T::Context),
}

impl<T: Form> FormResult<T> {
    /// Unwraps the form result, panicking if the form validation failed.
    ///
    /// This should only be used in tests or in cases where the form validation
    /// is guaranteed to pass.
    ///
    /// # Panics
    ///
    /// Panics if the form validation failed.
    pub fn unwrap(self) -> T {
        match self {
            Self::Ok(form) => form,
            Self::ValidationError(context) => panic!("Form validation failed: {context:?}"),
        }
    }
}

/// An error that can occur when validating a form field.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{message}")]
pub enum FormFieldValidationError {
    /// The field is required.
    #[error("This field is required.")]
    Required,
    /// The field value is too long.
    #[error("This exceeds the maximum length of {max_length}.")]
    MaximumLengthExceeded {
        /// The maximum length of the field.
        max_length: u32,
    },
    /// The field value is required to be true.
    #[error("This field must be checked.")]
    BooleanRequiredToBeTrue,
    /// The field value is invalid.
    #[error("Value is not valid for this field.")]
    InvalidValue(String),
    /// Custom error with given message.
    #[error("{0}")]
    Custom(Cow<'static, str>),
}

impl FormFieldValidationError {
    /// Creates a new `FormFieldValidationError` for an invalid value of a
    /// field.
    #[must_use]
    pub fn invalid_value<T: Into<String>>(value: T) -> Self {
        Self::InvalidValue(value.into())
    }

    /// Creates a new `FormFieldValidationError` for a field value that is too
    /// long.
    #[must_use]
    pub fn maximum_length_exceeded(max_length: u32) -> Self {
        Self::MaximumLengthExceeded { max_length }
    }

    /// Creates a new `FormFieldValidationError` from a `String`.
    #[must_use]
    pub const fn from_string(message: String) -> Self {
        Self::Custom(Cow::Owned(message))
    }

    /// Creates a new `FormFieldValidationError` from a static string.
    #[must_use]
    pub const fn from_static(message: &'static str) -> Self {
        Self::Custom(Cow::Borrowed(message))
    }
}

/// An enum indicating the target of a form validation error.
#[derive(Debug)]
pub enum FormErrorTarget<'a> {
    /// An error targetting a single field.
    Field(&'a str),
    /// An error targetting the entire form.
    Form,
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
/// use cot::form::Form;
///
/// #[derive(Form)]
/// struct MyForm {
///     #[form(opt(max_length = 100))]
///     name: String,
/// }
/// ```
#[async_trait]
pub trait Form: Sized {
    /// The context type associated with the form.
    type Context: FormContext;

    /// Creates a form struct from a request.
    ///
    /// # Errors
    ///
    /// This method should return an error if the form data could not be read
    /// from the request.
    async fn from_request(request: &mut Request) -> Result<FormResult<Self>, FormError>;

    /// Builds the context for the form from a request.
    ///
    /// Note that this doesn't try to convert the values from the form fields
    /// into the final types, so this context object may not include all the
    /// errors. The conversion is done in the [`Self::from_request`] method.
    ///
    /// # Errors
    ///
    /// This method should return an error if the form data could not be read
    /// from the request.
    async fn build_context(request: &mut Request) -> Result<Self::Context, FormError> {
        let form_data = request
            .form_data()
            .await
            .map_err(|error| FormError::RequestError {
                error: Box::new(error),
            })?;

        let mut context = Self::Context::new();

        for (field_id, value) in request::query_pairs(&form_data) {
            let field_id = field_id.as_ref();

            if let Err(err) = context.set_value(field_id, value) {
                context.add_error(FormErrorTarget::Field(field_id), err);
            }
        }

        Ok(context)
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
pub trait FormContext: Debug {
    /// Creates a new form context without any initial form data.
    fn new() -> Self
    where
        Self: Sized;

    /// Returns an iterator over the fields in the form.
    fn fields(&self)
        -> impl DoubleEndedIterator<Item = &dyn DynFormField> + ExactSizeIterator + '_;

    /// Sets the value of a form field.
    ///
    /// # Errors
    ///
    /// This method should return an error if the value is invalid.
    fn set_value(
        &mut self,
        field_id: &str,
        value: Cow<'_, str>,
    ) -> Result<(), FormFieldValidationError>;

    /// Adds a validation error to the form context.
    fn add_error(&mut self, target: FormErrorTarget<'_>, error: FormFieldValidationError) {
        self.errors_for_mut(target).push(error);
    }

    /// Returns the validation errors for a target in the form context.
    fn errors_for(&self, target: FormErrorTarget<'_>) -> &[FormFieldValidationError];

    /// Returns a mutable reference to the validation errors for a target in the
    /// form context.
    fn errors_for_mut(&mut self, target: FormErrorTarget<'_>)
        -> &mut Vec<FormFieldValidationError>;

    /// Returns whether the form context has any validation errors.
    fn has_errors(&self) -> bool;
}

impl<T: FormContext> Render for T {
    fn render(&self) -> Html {
        let mut html = String::new();

        for field in self.fields() {
            html.push_str(field.dyn_render().as_str());
        }

        Html::new(html)
    }
}

/// Generic options valid for all types of form fields.
#[derive(Debug)]
pub struct FormFieldOptions {
    /// The HTML ID of the form field.
    pub id: String,
    /// Whether the field is required. Note that this really only adds
    /// "required" field to the HTML input element, since by default all
    /// fields are required. If you want to make a field optional, just use
    /// [`Option`] in the struct definition.
    pub required: bool,
}

/// A form field.
///
/// This trait is used to define a type of field that can be used in a form. It
/// is used to render the field in an HTML form, set the value of the field, and
/// validate it. Typically, the implementors of this trait are used indirectly
/// through the [`Form`] trait and field types that implement [`AsFormField`].
pub trait FormField: Render {
    /// Custom options for the form field, unique for each field type.
    type CustomOptions: Default;

    /// Creates a new form field with the given options.
    fn with_options(options: FormFieldOptions, custom_options: Self::CustomOptions) -> Self
    where
        Self: Sized;

    /// Returns the generic options for the form field.
    fn options(&self) -> &FormFieldOptions;

    /// Returns the ID of the form field.
    fn id(&self) -> &str {
        &self.options().id
    }

    /// Returns the string value of the form field.
    fn value(&self) -> Option<&str>;

    /// Sets the string value of the form field.
    ///
    /// This method should convert the value to the appropriate type for the
    /// field, such as a number for a number field.
    fn set_value(&mut self, value: Cow<'_, str>);
}

/// A version of [`FormField`] that can be used in a dynamic context.
///
/// This trait is used to allow a form field to be used in a dynamic context,
/// such as when using Form field iterator. It provides access to the field's
/// options, value, and rendering, among others.
///
/// This trait is implemented for all types that implement [`FormField`].
pub trait DynFormField {
    /// Returns the generic options for the form field.
    fn dyn_options(&self) -> &FormFieldOptions;

    /// Returns the HTML ID of the form field.
    fn dyn_id(&self) -> &str;

    /// Sets the string value of the form field.
    fn dyn_set_value(&mut self, value: Cow<'_, str>);

    /// Renders the form field as HTML.
    fn dyn_render(&self) -> Html;
}

impl<T: FormField> DynFormField for T {
    fn dyn_options(&self) -> &FormFieldOptions {
        FormField::options(self)
    }

    fn dyn_id(&self) -> &str {
        FormField::id(self)
    }

    fn dyn_set_value(&mut self, value: Cow<'_, str>) {
        FormField::set_value(self, value);
    }

    fn dyn_render(&self) -> Html {
        Render::render(self)
    }
}

/// A trait for types that can be used as form fields.
///
/// This trait uses [`FormField`] to define a type that can be used as a form
/// field. It provides a way to clean the value of the field, which is used to
/// validate the field's value before converting to the final type.
pub trait AsFormField {
    /// The form field type associated with the field.
    type Type: FormField;

    /// Creates a new form field with the given options and custom options.
    ///
    /// This method is used to create a new instance of the form field with the
    /// given options and custom options. The options are used to set the
    /// properties of the field, such as the ID and whether the field is
    /// required.
    ///
    /// The custom options are unique to each field type and are used to set
    /// additional properties of the field.
    fn new_field(
        options: FormFieldOptions,
        custom_options: <Self::Type as FormField>::CustomOptions,
    ) -> Self::Type {
        Self::Type::with_options(options, custom_options)
    }

    /// Validates the value of the field and converts it to the final type. This
    /// method should return an error if the value is invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if the value fails to validate or convert to the final
    /// type
    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError>
    where
        Self: Sized;
}
