use flareon::db::{model, LimitedString};

pub const FIELD_LEN: u32 = 64;

#[model]
struct MyModel {
    field_1: String,
    field_2: LimitedString<FIELD_LEN>,
}

fn main() {}
