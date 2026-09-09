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

mod custom {
    use std::marker::PhantomData;

    pub struct Vec<T> {
        values: std::vec::Vec<String>,
        marker: PhantomData<T>,
    }

    impl<T> Vec<T> {
        pub fn new(values: std::vec::Vec<String>) -> Self {
            Self {
                values,
                marker: PhantomData,
            }
        }

        pub fn iter(&self) -> std::slice::Iter<'_, String> {
            self.values.iter()
        }
    }
}

#[derive(PassportPrincipal)]
struct CustomCollectionPrincipal {
    #[roles]
    roles: custom::Vec<u64>,
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

    let custom_collection = CustomCollectionPrincipal {
        roles: custom::Vec::new(vec!["auditor".into()]),
    };
    assert!(custom_collection.has_role("auditor"));
}
