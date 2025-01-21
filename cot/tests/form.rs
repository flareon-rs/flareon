use cot::form::{
    Form, FormContext, FormErrorTarget, FormField, FormFieldValidationError, FormResult,
};
use cot::test::TestRequestBuilder;

#[derive(Debug, Form)]
struct MyForm {
    name: String,
    address: Option<String>,
    age: u8,
}

#[tokio::test]
async fn context_from_empty_request() {
    let mut request = TestRequestBuilder::get("/").build();

    let context = MyForm::build_context(&mut request).await;
    assert!(context.is_ok());
}

#[tokio::test]
async fn context_display_non_empty() {
    let mut request = TestRequestBuilder::get("/").build();

    let context = MyForm::build_context(&mut request).await.unwrap();
    let form_rendered = context.to_string();
    assert!(!form_rendered.is_empty());
}

#[tokio::test]
async fn form_from_request() {
    let mut request = TestRequestBuilder::post("/")
        .form_data(&[("name", "Alice"), ("age", "30")])
        .build();

    let form = MyForm::from_request(&mut request).await.unwrap().unwrap();
    assert_eq!(form.name, "Alice");
    assert_eq!(form.address, None);
    assert_eq!(form.age, 30);
}

#[tokio::test]
async fn form_errors_required() {
    let mut request = TestRequestBuilder::post("/")
        .form_data::<String>(&[])
        .build();

    let form = MyForm::from_request(&mut request).await;
    match form {
        Ok(FormResult::ValidationError(context)) => {
            assert_eq!(context.errors_for(FormErrorTarget::Form), &[]);
            assert_eq!(
                context.errors_for(FormErrorTarget::Field("name")),
                &[FormFieldValidationError::Required]
            );
            assert_eq!(context.errors_for(FormErrorTarget::Field("address")), &[]);
            assert_eq!(
                context.errors_for(FormErrorTarget::Field("age")),
                &[FormFieldValidationError::Required]
            );
        }
        _ => panic!("Expected a validation error"),
    }
}

#[tokio::test]
async fn values_persist_on_form_errors() {
    let mut request = TestRequestBuilder::post("/")
        .form_data(&[("name", "Alice"), ("age", "invalid")])
        .build();

    let form = MyForm::from_request(&mut request).await;
    match form {
        Ok(FormResult::ValidationError(context)) => {
            assert_eq!(context.name.value(), Some("Alice"));
            assert_eq!(context.age.value(), Some("invalid"));

            assert_eq!(context.errors_for(FormErrorTarget::Form), &[]);
            assert_eq!(context.errors_for(FormErrorTarget::Field("name")), &[]);
            assert_eq!(context.errors_for(FormErrorTarget::Field("address")), &[]);
            assert_eq!(
                context.errors_for(FormErrorTarget::Field("age")),
                &[FormFieldValidationError::InvalidValue(
                    "invalid".to_string()
                )]
            );
        }
        _ => panic!("Expected a validation error"),
    }
}
