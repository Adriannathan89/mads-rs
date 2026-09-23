struct Resource;

#[mads::provider(lifecycle)]
async fn wrong_output() -> Resource {
    Resource
}

#[mads::provider(lifecycle)]
async fn generic_resource<T>() -> mads::core::LifecycleResource<T> {
    todo!()
}

fn main() {}
