use mads::prelude::*;
use mads_persistence::sea_orm::DatabaseConnection;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DbErr, EntityTrait, QueryOrder};

use super::model::{Post, entity};

#[repository]
pub struct PostRepository {
    database: DatabaseConnection,
}

impl PostRepository {
    pub async fn create(&self, title: String, body: String) -> Result<Post, DbErr> {
        let model = entity::ActiveModel {
            title: Set(title),
            body: Set(body),
            ..Default::default()
        }
        .insert(&self.database)
        .await?;
        Ok(model.into())
    }

    pub async fn list(&self) -> Result<Vec<Post>, DbErr> {
        let models = entity::Entity::find()
            .order_by_asc(entity::Column::Id)
            .all(&self.database)
            .await?;
        Ok(models.into_iter().map(Into::into).collect())
    }

    pub async fn find(&self, id: i32) -> Result<Option<Post>, DbErr> {
        Ok(entity::Entity::find_by_id(id)
            .one(&self.database)
            .await?
            .map(Into::into))
    }

    pub async fn update(
        &self,
        id: i32,
        title: String,
        body: String,
    ) -> Result<Option<Post>, DbErr> {
        let Some(model) = entity::Entity::find_by_id(id).one(&self.database).await? else {
            return Ok(None);
        };
        let mut active: entity::ActiveModel = model.into();
        active.title = Set(title);
        active.body = Set(body);
        Ok(Some(active.update(&self.database).await?.into()))
    }

    pub async fn delete(&self, id: i32) -> Result<bool, DbErr> {
        let result = entity::Entity::delete_by_id(id)
            .exec(&self.database)
            .await?;
        Ok(result.rows_affected > 0)
    }
}
