use std::sync::Arc;

use mads::prelude::*;

use super::{
    model::{LoginInput, ProfileResponse, TokenResponse, UserPrincipal},
    traits::AuthService,
};

#[routes(prefix = "/auth")]
pub trait AuthRoutes {
    #[post("/login")]
    async fn login(&self, input: ValidatedJson<LoginInput>) -> HttpResult<Json<TokenResponse>>;

    #[get("/me")]
    #[guard(strategy = "jwt", principal = UserPrincipal, source = bearer, roles(any = ["reader"]))]
    async fn me(
        &self,
        principal: Authenticated<UserPrincipal>,
    ) -> HttpResult<Json<ProfileResponse>>;
}

// Controller: only translates HTTP input/output and invokes the service.
#[controller(routes = [AuthRoutes])]
pub struct AuthController {
    service: Arc<dyn AuthService>,
    logger: Logger,
}

impl AuthRoutes for AuthController {
    async fn login(
        &self,
        ValidatedJson(input): ValidatedJson<LoginInput>,
    ) -> HttpResult<Json<TokenResponse>> {
        let token = self
            .service
            .login(&input.username, &input.password)
            .map_err(InternalError::new)?
            .ok_or_else(|| Unauthorized::new("invalid credentials"))?;
        Ok(Json(TokenResponse {
            access_token: token,
        }))
    }

    async fn me(
        &self,
        principal: Authenticated<UserPrincipal>,
    ) -> HttpResult<Json<ProfileResponse>> {
        self.logger.info("protected profile read");
        Ok(Json(ProfileResponse {
            id: principal.id,
            username: principal.username.clone(),
        }))
    }
}
