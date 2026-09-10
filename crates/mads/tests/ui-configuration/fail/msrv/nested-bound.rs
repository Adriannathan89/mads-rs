use mads::core::Configuration;

struct NotConfiguration;

#[derive(Configuration)]
struct Parent { child: NotConfiguration }

fn main() {}
