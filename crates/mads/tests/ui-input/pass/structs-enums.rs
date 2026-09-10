use mads::prelude::*;

#[derive(Input)]
struct Unit;
#[derive(Input)]
struct Pair(#[validate(nonempty)] String, f64);
#[derive(Input)]
struct Generic<T, U, const N: usize> {
    #[validate(nested)]
    values: [T; N],
    unused: std::marker::PhantomData<U>,
}
#[derive(serde::Deserialize, Input)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum Message {
    Unit,
    Text(#[validate(nonempty)] String),
    Named { #[validate(email)] address: String },
}
fn main() {
    struct NoInput;
    assert!(Generic::<_, NoInput, 1> { values: [Unit], unused: std::marker::PhantomData }.validate().is_ok());
    assert!(Pair("hi".into(), 1.0).validate().is_ok());
    assert!(Message::Unit.validate().is_ok());
}
