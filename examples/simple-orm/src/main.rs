use flareon_orm as orm;
use orm::{model, Model};

#[model(test, test_val = 3)]
struct User {
    pub id: u128,
    pub email: String,
}

fn main() {
    println!("Hello, world!");
}
