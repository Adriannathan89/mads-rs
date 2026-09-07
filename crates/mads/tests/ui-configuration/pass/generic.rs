use mads::core::{Config, Configuration};

#[derive(Configuration)]
struct Generic<T>
where T: Configuration
{
    child: Option<T>,
}

#[derive(Configuration)]
struct Empty {}

#[derive(Configuration)]
struct Inline<T: Configuration> {
    child: T,
}

fn main() {
    assert!(Config::empty().parse::<Generic<Empty>>().unwrap().child.is_none());
    let _: Empty = Config::empty().parse::<Inline<Empty>>().unwrap().child;
}
