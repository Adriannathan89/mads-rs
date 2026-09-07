//! Verifies controller expansion through a renamed common dependency.

#[web::routes]
trait Routes {
    #[web::get("/")]
    async fn index(&self);
}

#[web::controller(routes = [Routes])]
struct Controller;

impl Routes for Controller {
    async fn index(&self) {}
}

async fn build_application() -> web::core::Result<()> {
    let application = web::core::Mads::builder().build().await?;
    let _router = web::build_router(&application)?;
    Ok(())
}

fn main() {
    use web::Input;
    #[derive(web::Input)]
    struct RequestInput { #[validate(email)] email: String }
    assert!(RequestInput { email: "user@example.com".into() }.validate().is_ok());
    let _ = build_application;
}
