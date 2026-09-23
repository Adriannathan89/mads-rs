struct Resource;

#[mads::provider(lifecycle)]
fn resource() -> mads::core::LifecycleResource<Resource> {
    mads::core::LifecycleResource::new(Resource)
}

fn main() {}
