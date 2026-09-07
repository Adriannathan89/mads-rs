use mads::prelude::*;

fn field(_: &str) -> ValidationResult { Ok(()) }
fn whole<T>(_: &Borrowed<'_, T>) -> ValidationResult { Ok(()) }
#[derive(Input)]
#[validate(custom = whole)]
struct Borrowed<'a, T> {
    #[validate(custom = field, length(min = 2))]
    text: &'a str,
    #[validate(nested)]
    inner: Vec<(T,)>,
}
struct Manual;
impl Input for Manual {
    fn validate(&self) -> ValidationResult { Ok(()) }
}
fn main() {
    assert!(Borrowed { text: "é🦀", inner: vec![(Manual,)] }.validate().is_ok());
}
