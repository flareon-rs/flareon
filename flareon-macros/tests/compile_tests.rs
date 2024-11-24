#[rustversion::attr(not(nightly), ignore)]
#[test]
fn derive_form() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/derive_form.rs");
}

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn attr_model() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/attr_model.rs");
    t.compile_fail("tests/ui/attr_model_migration_invalid_name.rs");
    t.compile_fail("tests/ui/attr_model_tuple.rs");
    t.compile_fail("tests/ui/attr_model_enum.rs");
    t.compile_fail("tests/ui/attr_model_generic.rs");
}

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn func_query() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/func_query.rs");
    t.compile_fail("tests/ui/func_query_double_op.rs");
    t.compile_fail("tests/ui/func_query_starting_op.rs");
    t.compile_fail("tests/ui/func_query_double_field.rs");
    t.compile_fail("tests/ui/func_query_invalid_field.rs");
}

#[rustversion::attr(not(nightly), ignore)]
#[test]
fn attr_main() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/attr_main.rs");
    t.compile_fail("tests/ui/attr_main_args.rs");
}
