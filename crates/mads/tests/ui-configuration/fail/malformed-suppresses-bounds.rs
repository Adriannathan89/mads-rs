use mads::Configuration;

struct NotConfiguration;

#[derive(Configuration)]
struct Malformed {
    #[config(rename)]
    name: String,
    child: NotConfiguration,
}

fn main() {}
