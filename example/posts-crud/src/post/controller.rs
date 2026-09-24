use mads::prelude::*;

use super::{
    model::{Post, PostInput},
    service::PostService,
};

#[routes(prefix = "/posts")]
pub trait PostRoutes {
    #[post("/")]
    async fn create(&self, body: ValidatedJson<PostInput>) -> HttpResult<Created<Json<Post>>>;

    #[get("/")]
    async fn list(&self) -> HttpResult<Json<Vec<Post>>>;

    #[get("/:id")]
    async fn find(&self, id: Path<i32>) -> HttpResult<Json<Post>>;

    #[put("/:id")]
    async fn update(&self, id: Path<i32>, body: ValidatedJson<PostInput>)
    -> HttpResult<Json<Post>>;

    #[delete("/:id")]
    async fn delete(&self, id: Path<i32>) -> HttpResult<NoContent>;
}

#[controller(routes = [PostRoutes])]
pub struct PostController {
    service: PostService,
}

impl PostRoutes for PostController {
    async fn create(
        &self,
        ValidatedJson(body): ValidatedJson<PostInput>,
    ) -> HttpResult<Created<Json<Post>>> {
        let post = self
            .service
            .create(body.title, body.body)
            .await
            .map_err(InternalError::new)?;
        Ok(Created(Json(post)))
    }

    async fn list(&self) -> HttpResult<Json<Vec<Post>>> {
        Ok(Json(self.service.list().await.map_err(InternalError::new)?))
    }

    async fn find(&self, Path(id): Path<i32>) -> HttpResult<Json<Post>> {
        self.service
            .find(id)
            .await
            .map_err(InternalError::new)?
            .map(Json)
            .ok_or_else(|| NotFound::new("post not found").into())
    }

    async fn update(
        &self,
        Path(id): Path<i32>,
        ValidatedJson(body): ValidatedJson<PostInput>,
    ) -> HttpResult<Json<Post>> {
        self.service
            .update(id, body.title, body.body)
            .await
            .map_err(InternalError::new)?
            .map(Json)
            .ok_or_else(|| NotFound::new("post not found").into())
    }

    async fn delete(&self, Path(id): Path<i32>) -> HttpResult<NoContent> {
        if self.service.delete(id).await.map_err(InternalError::new)? {
            Ok(NoContent)
        } else {
            Err(NotFound::new("post not found").into())
        }
    }
}
