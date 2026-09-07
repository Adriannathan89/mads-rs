use std::collections::BTreeSet;

use mads::common::PassportPrincipal;

#[derive(PassportPrincipal)]
struct OwnedVecPrincipal {
    #[roles]
    roles: Vec<String>,
}

#[derive(PassportPrincipal)]
struct BorrowedVecPrincipal {
    #[permissions]
    permissions: Vec<&'static str>,
}

#[derive(PassportPrincipal)]
struct OwnedSetPrincipal {
    #[roles]
    roles: BTreeSet<String>,
}

#[derive(PassportPrincipal)]
struct BorrowedSetPrincipal {
    #[permissions]
    permissions: BTreeSet<&'static str>,
}

struct RoleList(Vec<String>);

impl RoleList {
    fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }
}

#[derive(PassportPrincipal)]
struct CustomIteratorPrincipal {
    #[roles]
    roles: RoleList,
}

fn main() {
    let owned_vec = OwnedVecPrincipal {
        roles: vec!["admin".into()],
    };
    assert!(owned_vec.has_role("admin"));

    let borrowed_vec = BorrowedVecPrincipal {
        permissions: vec!["profile:read"],
    };
    assert!(borrowed_vec.has_permission("profile:read"));

    let owned_set = OwnedSetPrincipal {
        roles: ["editor".to_owned()].into_iter().collect(),
    };
    assert!(owned_set.has_role("editor"));

    let borrowed_set = BorrowedSetPrincipal {
        permissions: ["article:read"].into_iter().collect(),
    };
    assert!(borrowed_set.has_permission("article:read"));

    let custom_iterator = CustomIteratorPrincipal {
        roles: RoleList(vec!["operator".into()]),
    };
    assert!(custom_iterator.has_role("operator"));
}
