use mads::Input;

struct NotInput;
#[derive(Input)]
struct Parent { #[validate(nested)] child: NotInput }
fn main() {}
