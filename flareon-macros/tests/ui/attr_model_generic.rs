use flareon::db::model;

#[model]
struct MyModel<T> {
    id: i32,
    some_data: T,
}

fn main() {}
