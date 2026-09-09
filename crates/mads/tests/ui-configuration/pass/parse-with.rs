use mads::core::{Config, ConfigBuilder, Configuration, MapSource, Secret};

#[derive(Configuration)]
struct Generic<T: std::str::FromStr> {
    #[config(parse_with = str::parse)]
    value: T,
    #[config(parse_with = str::parse)]
    secret: Secret<T>,
}

fn main() {
    let config: Config = ConfigBuilder::new().source(MapSource::new("fixture", [
        ("value", "42"), ("secret", "43"),
    ])).build().unwrap();
    let value = config.parse::<Generic<u16>>().unwrap();
    assert_eq!(value.value, 42);
    assert_eq!(value.secret.expose(), &43);
}
