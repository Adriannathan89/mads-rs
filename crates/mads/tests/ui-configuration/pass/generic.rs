use mads::core::{Config, Configuration};

#[derive(Configuration)]
struct Generic<T>
where T: Configuration
{
    child: Option<T>,
}

#[derive(Configuration)]
struct Empty {}

fn main() {
    assert!(Config::empty().parse::<Generic<Empty>>().unwrap().child.is_none());
}
