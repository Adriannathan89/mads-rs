use mads::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct User {
    pub id: u64,
    pub username: String,
}

#[derive(Serialize, Deserialize)]
pub struct UserClaims {
    pub user_id: u64,
}

#[derive(PassportPrincipal)]
pub struct UserPrincipal {
    pub id: u64,
    pub username: String,
    #[roles]
    pub roles: Vec<String>,
}

#[derive(Deserialize, Input)]
pub struct LoginInput {
    #[validate(nonempty)]
    pub username: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub id: u64,
    pub username: String,
}
