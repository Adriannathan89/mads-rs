use mads::common::PassportPrincipal;

#[derive(PassportPrincipal)]
struct Principal {
    #[roles]
    roles: u64,
}

fn main() {}
