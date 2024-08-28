#[test]
fn test_derive_form() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/derive_form.rs");
}

#[test]
fn test_attr_model() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/attr_model.rs");
    t.compile_fail("tests/ui/attr_model_migration_invalid_name.rs");
}
