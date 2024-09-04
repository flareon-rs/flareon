use flareon::db::{model, query};

#[derive(Debug)]
#[model]
struct MyModel {
    id: i32,
    name: std::string::String,
    description: String,
    visits: i32,
}

fn main() {
    query!(
        MyModel,
        $name $name
    );
}
