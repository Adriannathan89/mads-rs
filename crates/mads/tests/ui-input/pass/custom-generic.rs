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
#[derive(Input)]
#[validate(custom = whole_trait_object)]
struct Coerced {
    #[validate(custom = field)]
    text: String,
}
trait Text {
    fn text(&self) -> &str;
}
impl Text for Coerced {
    fn text(&self) -> &str { &self.text }
}
fn whole_trait_object(value: &dyn Text) -> ValidationResult {
    assert_eq!(value.text(), "value");
    Ok(())
}
fn main() {
    assert!(Borrowed { text: "é🦀", inner: vec![(Manual,)] }.validate().is_ok());
    assert!(Coerced { text: "value".into() }.validate().is_ok());
}
