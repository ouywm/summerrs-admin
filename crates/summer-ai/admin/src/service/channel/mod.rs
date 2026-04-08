use std::collections::HashSet;

use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::channel::req::{ChannelQuery, CreateChannelReq, UpdateChannelReq};
use crate::router::channel::res::{ChannelDetailRes, ChannelListRes};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};

#[derive(Clone, Service)]
pub struct ChannelService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelService {
    pub async fn list_channels(
        &self,
        query: ChannelQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelListRes>> {
        let page = channel::Entity::find()
            .filter(query)
            .order_by_desc(channel::Column::Priority)
            .order_by_desc(channel::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道列表失败")?;

        Ok(page.map(ChannelListRes::from_model))
    }

    pub async fn get_channel(&self, id: i64) -> ApiResult<ChannelDetailRes> {
        let model = self.find_channel_model(id).await?;
        Ok(ChannelDetailRes::from_model(model))
    }

    pub async fn create_channel(&self, req: CreateChannelReq, operator: &str) -> ApiResult<()> {
        let model = req
            .into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建渠道失败")?;

        self.sync_abilities(&model).await?;
        Ok(())
    }

    pub async fn update_channel(
        &self,
        id: i64,
        req: UpdateChannelReq,
        operator: &str,
    ) -> ApiResult<()> {
        let mut active: channel::ActiveModel = self.find_channel_model(id).await?.into();
        req.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;

        let model = active.update(&self.db).await.context("更新渠道失败")?;
        self.sync_abilities(&model).await?;
        Ok(())
    }

    pub async fn delete_channel(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: channel::ActiveModel = self.find_channel_model(id).await?.into();
        active.status = Set(ChannelStatus::Archived);
        active.deleted_at = Set(Some(chrono::Utc::now().fixed_offset()));
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.context("删除渠道失败")?;

        ability::Entity::delete_many()
            .filter(ability::Column::ChannelId.eq(id))
            .exec(&self.db)
            .await
            .context("删除渠道能力失败")?;

        Ok(())
    }

    async fn find_channel_model(&self, id: i64) -> ApiResult<channel::Model> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道不存在".to_string()))
    }

    async fn sync_abilities(&self, model: &channel::Model) -> ApiResult<()> {
        ability::Entity::delete_many()
            .filter(ability::Column::ChannelId.eq(model.id))
            .exec(&self.db)
            .await
            .context("同步渠道能力前清理旧记录失败")?;

        if model.deleted_at.is_some() {
            return Ok(());
        }

        let abilities = self.build_abilities(model);
        if !abilities.is_empty() {
            ability::Entity::insert_many(abilities)
                .exec(&self.db)
                .await
                .context("同步渠道能力失败")?;
        }

        Ok(())
    }

    fn build_abilities(&self, model: &channel::Model) -> Vec<ability::ActiveModel> {
        let models = json_string_list(&model.models);
        let scopes = json_string_list(&model.endpoint_scopes);
        let mut pairs = HashSet::new();

        for model_name in models {
            for scope in &scopes {
                pairs.insert((model_name.clone(), scope.clone()));
            }
        }

        pairs
            .into_iter()
            .map(|(model_name, endpoint_scope)| ability::ActiveModel {
                channel_group: Set(model.channel_group.clone()),
                endpoint_scope: Set(endpoint_scope),
                model: Set(model_name),
                channel_id: Set(model.id),
                enabled: Set(model.status == ChannelStatus::Enabled),
                priority: Set(model.priority),
                weight: Set(model.weight),
                route_config: Set(serde_json::json!({})),
                ..Default::default()
            })
            .collect()
    }
}

fn json_string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
