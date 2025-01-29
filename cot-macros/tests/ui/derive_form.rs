use cot::form::Form;
use cot::request::Request;

#[derive(Debug, Form)]
struct MyForm {
    name: String,
    name2: std::string::String,
}

#[allow(unused)]
async fn test_endpoint(mut request: Request) {
    let form = MyForm::from_request(&mut request).await.unwrap().unwrap();
    println!("name = {}, name2 = {}", form.name, form.name2);
}

fn main() {}
