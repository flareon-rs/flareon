use flareon::db::{model, Auto, ForeignKey};

#[model]
struct Parent {
    id: Auto<i32>,
}

fn main() {}
