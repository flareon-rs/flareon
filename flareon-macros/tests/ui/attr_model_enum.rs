use flareon::db::model;

#[model]
enum MyModel {
    A(i32),
    B(std::string::String),
    C(String),
    D(i32),
}

fn main() {}
