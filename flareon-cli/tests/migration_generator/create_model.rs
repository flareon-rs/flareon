use flareon::db::{model, Auto, LimitedString};

pub const FIELD_LEN: u32 = 64;

#[model]
struct MyModel {
    id: Auto<i32>,
    field_1: String,
    field_2: LimitedString<FIELD_LEN>,
}

fn main() {}
