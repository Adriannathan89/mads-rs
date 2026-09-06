use mads::core::Configuration;

#[derive(Configuration)]
struct Tuple(String);
#[derive(Configuration)]
struct Unit;
#[derive(Configuration)]
enum Enum { Value }
#[derive(Configuration)]
union Union { value: u32 }
#[derive(Configuration)]
struct Collection { values: Vec<u32> }
#[derive(Configuration)]
struct Array { values: [String; 2] }
#[derive(Configuration)]
struct Map { values: std::collections::BTreeMap<String, String> }
#[derive(Configuration)]
struct Generic<T> { value: T }

fn main() {}
