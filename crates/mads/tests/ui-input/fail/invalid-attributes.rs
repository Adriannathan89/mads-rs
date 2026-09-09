use mads::Input;

#[derive(Input)]
struct Unknown { #[validate(trim)] value: String }
#[derive(Input)]
struct Duplicate { #[validate(email)] #[validate(email)] value: String }
#[derive(Input)]
struct Empty { #[validate()] value: String }
#[derive(Input)]
struct Bounds { #[validate(length(min = 2, min = 3))] value: String }
#[derive(Input)]
struct UnknownBound { #[validate(range(low = 1))] value: i32 }
#[derive(Input)]
struct MissingBounds { #[validate(length)] value: String }
#[derive(Input)]
struct MissingCallback { #[validate(custom)] value: String }
#[derive(Input)]
#[validate(email)]
struct Whole { value: String }
fn main() {}
