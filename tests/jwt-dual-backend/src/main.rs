use std::time::Duration;

use mads_common::core::{ConfigBuilder, MapSource};
use mads_common::{JwtService, JwtSignOptions, JwtValidation};

fn main() {
    let config = ConfigBuilder::new()
        .source(MapSource::new(
            "test",
            [("passport.secret", "01234567890123456789012345678901")],
        ))
        .build()
        .unwrap();
    let service = JwtService::from_config(&config).unwrap();
    let token = service
        .sign(
            serde_json::json!({ "user_id": 7 }),
            JwtSignOptions::access(Duration::from_secs(60)),
        )
        .unwrap();
    let verified = service
        .verify::<serde_json::Value>(&token, JwtValidation::access())
        .unwrap();
    assert_eq!(verified.claims.custom["user_id"], 7);
}
