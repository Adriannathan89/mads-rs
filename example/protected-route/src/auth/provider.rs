use std::sync::Arc;

use mads::prelude::*;

use super::{
    repository::DemoUserRepository,
    service::AuthServiceImpl,
    traits::{AuthService, UserRepository},
};

#[derive(Configuration)]
#[config(prefix = "demo")]
struct DemoConfig {
    username: String,
    password: Secret<String>,
}

// Provider: binds each trait to the implementation used by this application.
#[provider]
pub fn user_repository(config: Config) -> mads::core::Result<Arc<dyn UserRepository>> {
    let settings: DemoConfig = config.parse()?;
    Ok(Arc::new(DemoUserRepository::new(
        settings.username,
        settings.password,
    )))
}

#[provider]
pub fn auth_service(service: AuthServiceImpl) -> Arc<dyn AuthService> {
    Arc::new(service)
}
