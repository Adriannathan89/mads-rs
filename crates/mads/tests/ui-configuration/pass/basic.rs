use mads::core::{Config, Configuration};

#[derive(Configuration)]
struct Defaults {
    #[config(default = "localhost")]
    host: String,
    #[config(default = true)]
    enabled: bool,
    #[config(default = -12)]
    offset: i16,
    #[config(default = 'é')]
    letter: char,
    #[config(default = 1.5)]
    fraction: f32,
}

fn main() {
    let config = Config::empty().parse::<Defaults>().unwrap();
    assert_eq!(config.host, "localhost");
    assert!(config.enabled);
    assert_eq!(config.offset, -12);
    assert_eq!(config.letter, 'é');
    assert_eq!(config.fraction, 1.5);
}
