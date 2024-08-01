use std::borrow::Cow;

use async_trait::async_trait;
pub use flareon_macros::Form;
use thiserror::Error;

use crate::request::Request;

#[derive(Debug, Error)]
pub enum FormError<T: Form> {
    #[error("Request error: {error}")]
    RequestError {
        #[from]
        error: crate::Error,
    },
    #[error("...")]
    ValidationError { context: T::Context },
}

const FORM_FIELD_REQUIRED: &str = "This field is required.";

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
    #[must_use]
    pub const fn from_string(message: String) -> Self {
        Self {
            message: Cow::Owned(message),
        }
    }

    #[must_use]
    pub const fn from_static(message: &'static str) -> Self {
        Self {
            message: Cow::Borrowed(message),
        }
    }
}

#[async_trait]
pub trait Form: Sized {
    type Context: FormContext;

    async fn from_request(request: &mut Request) -> Result<Self, FormError<Self>>;

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

pub trait FormContext: Sized {
    fn new() -> Self;

    fn fields(&self) -> impl Iterator<Item = &dyn FormField> + '_;

    fn set_value(
        &mut self,
        field_id: &str,
        value: Cow<str>,
    ) -> Result<(), FormFieldValidationError>;

    fn add_error(&mut self, target: FormErrorTarget, error: FormFieldValidationError) {
        self.get_errors_mut(target).push(error);
    }

    fn get_errors(&self, target: FormErrorTarget) -> &[FormFieldValidationError];

    fn get_errors_mut(&mut self, target: FormErrorTarget) -> &mut Vec<FormFieldValidationError>;
}

#[derive(Debug)]
pub struct FormFieldOptions {
    pub id: String,
}

pub trait FormField {
    fn options(&self) -> &FormFieldOptions;

    fn id(&self) -> &str {
        &self.options().id
    }

    fn set_value(&mut self, value: Cow<str>);

    fn render(&self) -> String;
}

pub trait AsFormField {
    type Type: FormField;

    fn with_options(options: FormFieldOptions) -> Self::Type;

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError>
    where
        Self: Sized;
}

#[derive(Debug)]
pub struct CharField {
    options: FormFieldOptions,
    value: Option<String>,
}

impl CharField {
    pub fn with_options(options: FormFieldOptions) -> Self {
        Self {
            options,
            value: None,
        }
    }
}

impl FormField for CharField {
    fn options(&self) -> &FormFieldOptions {
        &self.options
    }

    fn set_value(&mut self, value: Cow<str>) {
        self.value = Some(value.into_owned());
    }
}

impl AsFormField for String {
    type Type = CharField;

    fn with_options(options: FormFieldOptions) -> Self::Type {
        CharField::with_options(options)
    }

    fn clean_value(field: &Self::Type) -> Result<Self, FormFieldValidationError> {
        if let Some(value) = &field.value {
            Ok(value.clone())
        } else {
            Err(FormFieldValidationError::from_static(FORM_FIELD_REQUIRED))
        }
    }
}

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

    fn id(&mut self, id: &str) -> &mut Self {
        self.attr("id", id)
    }

    fn attr(&mut self, key: &str, value: &str) -> &mut Self {
        if self.attributes.iter().any(|(k, _)| k == key) {
            panic!("Attribute already exists: {}", key);
        }
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }

    #[must_use]
    fn render(&self) -> String {
        let mut result = format!("<{} ", self.tag);

        for (key, value) in &self.attributes {
            result.push_str(&format!("{}=\"{}\" ", key, value));
        }

        result.push_str(" />");
        result
    }
}
