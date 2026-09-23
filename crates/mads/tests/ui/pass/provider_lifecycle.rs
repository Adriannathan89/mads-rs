use mads::core::LifecycleResource;

struct DirectResource;
struct FallibleResource;

#[mads::provider(lifecycle)]
async fn direct_resource() -> LifecycleResource<DirectResource> {
    LifecycleResource::new(DirectResource)
}

#[mads::provider(lifecycle)]
async fn fallible_resource() -> mads::core::Result<LifecycleResource<FallibleResource>> {
    Ok(LifecycleResource::new(FallibleResource))
}

fn main() {}
