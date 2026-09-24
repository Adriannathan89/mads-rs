use mads::prelude::JwtResult;

use super::model::User;

// Trait: application contracts independent of HTTP and storage details.
pub trait UserRepository: Send + Sync {
    fn authenticate(&self, username: &str, password: &str) -> Option<User>;
    fn find_by_id(&self, id: u64) -> Option<User>;
}

pub trait AuthService: Send + Sync {
    fn login(&self, username: &str, password: &str) -> JwtResult<Option<String>>;
    fn find_by_id(&self, id: u64) -> Option<User>;
}
