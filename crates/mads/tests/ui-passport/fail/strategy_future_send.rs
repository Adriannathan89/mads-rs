use std::rc::Rc;

use mads::common::*;

struct NonSendingStrategy;

// Keep this diagnostic on a two-digit line so rustc 1.94 and current stable
// render the trybuild gutter identically.
//
#[passport_strategy(name = "jwt")]
impl PassportStrategy for NonSendingStrategy {
    type Claims = ();
    type Principal = Principal;
    const TOKEN_KIND: JwtTokenKind = JwtTokenKind::Access;

    async fn validate(
        &self,
        _context: &PassportContext<'_>,
        _claims: &JwtClaims<Self::Claims>,
    ) -> PassportResult<Self::Principal> {
        let local = Rc::new(());
        std::future::pending::<()>().await;
        let _ = local;
        Ok(Principal)
    }
}

struct Principal;

impl PassportPrincipal for Principal {
    fn has_role(&self, _role: &str) -> bool {
        false
    }

    fn has_permission(&self, _permission: &str) -> bool {
        false
    }
}

fn main() {}
