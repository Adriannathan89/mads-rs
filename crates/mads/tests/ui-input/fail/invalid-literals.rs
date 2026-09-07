use mads::Input;

#[derive(Input)]
struct Zero { #[validate(multiple_of = 0)] value: i32 }
#[derive(Input)]
struct Overflow { #[validate(range(max = 256))] value: u8 }
#[derive(Input)]
struct FloatOverflow { #[validate(range(max = 1e100))] value: f32 }
#[derive(Input)]
struct Reversed { #[validate(range(min = 4, max = 3))] value: u128 }
#[derive(Input)]
struct Exact { #[validate(length(exact = 3, min = 2))] value: String }
#[derive(Input)]
struct Contradictory { #[validate(nonempty)] #[validate(length(max = 0))] value: String }
#[derive(Input)]
struct Sign { #[validate(positive, negative)] value: i32 }
#[derive(Input)]
struct FloatLength { #[validate(length(min = 1.5))] value: String }
fn main() {}
