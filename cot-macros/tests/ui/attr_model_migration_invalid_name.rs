use cot::db::model;

#[model(model_type = "migration")]
struct MyModel {
    id: i32,
    name: std::string::String,
    description: String,
    visits: i32,
}

fn main() {}
