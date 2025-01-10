use cot::db::{model, Auto, ForeignKey, LimitedString};

pub const FIELD_LEN: u32 = 64;

#[derive(Debug)]
#[model]
struct Parent {
    id: Auto<i32>,
}

#[derive(Debug)]
#[model]
struct MyModel {
    id: Auto<i32>,
    field_1: String,
    field_2: LimitedString<FIELD_LEN>,
    parent: ForeignKey<Parent>,
}

fn main() {}
