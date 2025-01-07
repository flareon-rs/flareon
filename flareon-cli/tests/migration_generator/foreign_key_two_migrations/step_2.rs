use flareon::db::{model, Auto, ForeignKey};

#[model]
struct Child {
    id: Auto<i32>,
    parent: ForeignKey<Parent>,
}

#[model]
struct Parent {
    id: Auto<i32>,
}

fn main() {}
