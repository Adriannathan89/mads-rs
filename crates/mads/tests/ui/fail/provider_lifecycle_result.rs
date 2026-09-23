struct Resource;
struct CustomError;

#[mads::provider(lifecycle)]
async fn resource() -> std::result::Result<mads::core::LifecycleResource<Resource>, CustomError> {
    Ok(mads::core::LifecycleResource::new(Resource))
}

fn main() {}
