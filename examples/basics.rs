use std::collections::HashMap;
struct User {
    name: String,
    visits: u32,
}
trait Greeter {
    fn greet(&self) -> String;
}
impl Greeter for User {
    fn greet(&self) -> String {
        format!("Hello {}", self.name)
    }
}
impl User {
    fn increment(&mut self) {
        self.visits += 1;
    }
}
fn main() {
    let mut user = User {
        name: "Java developer".into(),
        visits: 0,
    };
    user.increment();
    let mut scores: HashMap<String, i32> = HashMap::new();
    scores.insert(user.name.clone(), 100);
    let value: Option<&i32> = scores.get(&user.name);
    println!("{} {:?}", user.greet(), value);
}
