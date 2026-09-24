use std::{sync::Arc, time::Duration};

use mads::prelude::*;

use super::{
    model::{User, UserClaims, UserPrincipal},
    traits::{AuthService, UserRepository},
};

// Service: login and identity lookup are application use cases.
#[service]
pub struct AuthServiceImpl {
    repository: Arc<dyn UserRepository>,
    jwt: JwtService,
    logger: Logger,
}

impl AuthService for AuthServiceImpl {
    fn login(&self, username: &str, password: &str) -> JwtResult<Option<String>> {
        let Some(user) = self.repository.authenticate(username, password) else {
            self.logger.warn("login rejected");
            return Ok(None);
        };

        let token = self.jwt.sign(
            UserClaims { user_id: user.id },
            JwtSignOptions::access(Duration::from_secs(15 * 60)).subject(user.id.to_string()),
        )?;
        self.logger.info("login succeeded");
        Ok(Some(token))
    }

    fn find_by_id(&self, id: u64) -> Option<User> {
        self.repository.find_by_id(id)
    }
}

#[service]
pub struct DemoJwtStrategy {
    service: Arc<dyn AuthService>,
}

#[passport_strategy(name = "jwt")]
impl PassportStrategy for DemoJwtStrategy {
    type Claims = UserClaims;
    type Principal = UserPrincipal;
    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        _context: &PassportContext<'_>,
        claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        self.service
            .find_by_id(claims.custom.user_id)
            .map(|user| UserPrincipal {
                id: user.id,
                username: user.username,
                roles: vec!["reader".to_string()],
            })
            .ok_or_else(PassportError::reject)
    }
}
