use mads::core::Configuration;

#[derive(Configuration)]
#[config(prefix = "app.server")]
struct DottedPrefix { port: u16 }
#[derive(Configuration)]
#[config(prefix = "one")]
#[config(prefix = "two")]
struct DuplicatePrefix { port: u16 }
#[derive(Configuration)]
struct EmptyKey { #[config(rename = "")] port: u16 }
#[derive(Configuration)]
struct DottedKey { #[config(rename = "app.port")] port: u16 }
#[derive(Configuration)]
struct Duplicate {
    port: u16,
    #[config(rename = "port")]
    other: u16,
}
#[derive(Configuration)]
struct DuplicateAttribute { #[config(rename = "one", rename = "two")] port: u16 }
#[derive(Configuration)]
struct Unknown { #[config(optional)] port: u16 }

fn main() {}
