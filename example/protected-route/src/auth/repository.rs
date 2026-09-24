use mads::prelude::Secret;

use super::{model::User, traits::UserRepository};

// Repository: deliberately in memory so this project focuses on auth and TPRS.
pub struct DemoUserRepository {
    username: String,
    password: Secret<String>,
}

impl DemoUserRepository {
    pub fn new(username: String, password: Secret<String>) -> Self {
        Self { username, password }
    }
}

impl UserRepository for DemoUserRepository {
    fn authenticate(&self, username: &str, password: &str) -> Option<User> {
        (self.username == username && self.password.expose() == password).then(|| User {
            id: 1,
            username: self.username.clone(),
        })
    }

    fn find_by_id(&self, id: u64) -> Option<User> {
        (id == 1).then(|| User {
            id,
            username: self.username.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_credentials_and_unknown_user_id() {
        let repository = DemoUserRepository::new("demo".into(), Secret::new("password123".into()));
        assert!(repository.authenticate("demo", "wrong-password").is_none());
        assert!(repository.authenticate("other", "password123").is_none());
        assert!(repository.find_by_id(2).is_none());
        assert_eq!(
            repository.authenticate("demo", "password123").unwrap().id,
            1
        );
    }
}
