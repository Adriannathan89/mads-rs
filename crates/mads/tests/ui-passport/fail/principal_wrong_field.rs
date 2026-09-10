use mads::common::PassportPrincipal;

#[derive(PassportPrincipal)]
struct Principal {
    #[roles]
    roles: std::vec::Vec<u64>,
}

fn main() {}
