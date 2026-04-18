pub struct Greeter;

pub fn greet(name: &str) -> String {
	format!("hello {name}")
}

pub fn shout(name: &str) -> String {
	format!("HELLO {}", name.to_uppercase())
}
