use cot::db::{model, Model};

#[derive(Debug)]
#[model]
struct MyModel {
    id: i32,
    name: std::string::String,
    description: String,
    visits: i32,
}

fn main() {
    println!("{:?}", MyModel::TABLE_NAME);
}
