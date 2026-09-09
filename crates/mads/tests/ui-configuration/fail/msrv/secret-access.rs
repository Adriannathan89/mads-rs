use mads::Secret;

fn main() {
    let secret = Secret::new(String::from("sentinel"));
    let _: &String = &*secret;
    let _: &String = AsRef::<String>::as_ref(&secret);
    let _: &String = std::borrow::Borrow::<String>::borrow(&secret);
    let _ = serde_json::to_string(&secret);
}
