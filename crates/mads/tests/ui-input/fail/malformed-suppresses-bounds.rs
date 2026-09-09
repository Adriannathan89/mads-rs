use mads::Input;

struct NotInput;

#[derive(Input)]
struct Malformed {
    #[validate(length(min))]
    name: String,
    #[validate(nested)]
    child: NotInput,
}

fn main() {}
