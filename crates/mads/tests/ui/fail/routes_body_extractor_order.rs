//! Rejects a known MADS/Axum body extractor before a part extractor.

#[mads::routes]
trait Routes {
    #[mads::post("/:id")]
    async fn create(&self, payload: mads_common::Json<String>, id: mads::common::Path<u64>);
}

fn main() {}
