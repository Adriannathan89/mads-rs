use sea_orm::DbErr;

use super::{model::Post, repository::PostRepository};
use mads::prelude::*;

#[service]
pub struct PostService {
    repository: PostRepository,
}

impl PostService {
    pub async fn create(&self, title: String, body: String) -> Result<Post, DbErr> {
        self.repository.create(title, body).await
    }

    pub async fn list(&self) -> Result<Vec<Post>, DbErr> {
        self.repository.list().await
    }

    pub async fn find(&self, id: i32) -> Result<Option<Post>, DbErr> {
        self.repository.find(id).await
    }

    pub async fn update(
        &self,
        id: i32,
        title: String,
        body: String,
    ) -> Result<Option<Post>, DbErr> {
        self.repository.update(id, title, body).await
    }

    pub async fn delete(&self, id: i32) -> Result<bool, DbErr> {
        self.repository.delete(id).await
    }
}
