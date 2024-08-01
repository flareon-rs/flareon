#[test]
fn test_derive_form() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/derive_form.rs");
}
