//! Rejects multiple known MADS/Axum body consumers in one route.

use serde::Deserialize;

#[derive(Deserialize, mads::Input)]
struct CreateUser {
    email: String,
}

#[mads::routes]
trait Routes {
    #[mads::post("/")]
    async fn create(&self, payload: mads::ValidatedJson<CreateUser>, request: mads::Request);
}

fn main() {}
