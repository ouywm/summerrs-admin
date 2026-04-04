use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::conversation::{
    CreateConversationDto, CreateMessageDto, CreatePromptTemplateDto, QueryConversationDto,
    QueryMessageDto, QueryPromptTemplateDto, UpdateConversationDto, UpdatePromptTemplateDto,
};
use summer_ai_model::entity::conversation;
use summer_ai_model::entity::message;
use summer_ai_model::entity::prompt_template;
use summer_ai_model::vo::conversation::{
    ConversationDetailVo, ConversationVo, MessageVo, PromptTemplateVo,
};

#[derive(Clone, Service)]
pub struct ConversationService {
    #[inject(component)]
    db: DbConn,
}

impl ConversationService {
    // ─── Conversation ───

    pub async fn list_conversations(
        &self,
        query: QueryConversationDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ConversationVo>> {
        let page = conversation::Entity::find()
            .filter(query)
            .order_by_desc(conversation::Column::UpdateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询对话列表失败")?;
        Ok(page.map(ConversationVo::from_model))
    }

    pub async fn get_conversation(&self, id: i64) -> ApiResult<ConversationDetailVo> {
        let model = conversation::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询对话失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("对话不存在".to_string()))?;
        Ok(ConversationDetailVo::from_model(model))
    }

    pub async fn create_conversation(
        &self,
        dto: CreateConversationDto,
    ) -> ApiResult<ConversationVo> {
        let model = dto
            .into_active_model()
            .insert(&self.db)
            .await
            .context("创建对话失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ConversationVo::from_model(model))
    }

    pub async fn update_conversation(
        &self,
        id: i64,
        dto: UpdateConversationDto,
    ) -> ApiResult<ConversationVo> {
        let model = conversation::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询对话失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("对话不存在".to_string()))?;
        let mut active: conversation::ActiveModel = model.into();
        dto.apply_to(&mut active);
        let updated = active
            .update(&self.db)
            .await
            .context("更新对话失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ConversationVo::from_model(updated))
    }

    pub async fn delete_conversation(&self, id: i64) -> ApiResult<()> {
        conversation::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除对话失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── Message ───

    pub async fn list_messages(
        &self,
        query: QueryMessageDto,
        pagination: Pagination,
    ) -> ApiResult<Page<MessageVo>> {
        let page = message::Entity::find()
            .filter(query)
            .order_by_asc(message::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询消息列表失败")?;
        Ok(page.map(MessageVo::from_model))
    }

    pub async fn create_message(&self, dto: CreateMessageDto) -> ApiResult<MessageVo> {
        let conv_id = dto.conversation_id;
        let model = dto
            .into_active_model()
            .insert(&self.db)
            .await
            .context("创建消息失败")
            .map_err(ApiErrors::Internal)?;

        // 更新对话的 message_count 和 last_message_at
        if let Ok(Some(conv)) = conversation::Entity::find_by_id(conv_id)
            .one(&self.db)
            .await
        {
            let new_count = conv.message_count + 1;
            let mut active: conversation::ActiveModel = conv.into();
            active.message_count = Set(new_count);
            active.last_message_at = Set(Some(chrono::Utc::now().fixed_offset()));
            let _ = active.update(&self.db).await;
        }

        Ok(MessageVo::from_model(model))
    }

    pub async fn delete_message(&self, id: i64) -> ApiResult<()> {
        message::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除消息失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── PromptTemplate ───

    pub async fn list_prompt_templates(
        &self,
        query: QueryPromptTemplateDto,
        pagination: Pagination,
    ) -> ApiResult<Page<PromptTemplateVo>> {
        let page = prompt_template::Entity::find()
            .filter(query)
            .order_by_asc(prompt_template::Column::TemplateSort)
            .order_by_desc(prompt_template::Column::UseCount)
            .page(&self.db, &pagination)
            .await
            .context("查询模板列表失败")?;
        Ok(page.map(PromptTemplateVo::from_model))
    }

    pub async fn get_prompt_template(&self, id: i64) -> ApiResult<PromptTemplateVo> {
        let model = prompt_template::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询模板失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("模板不存在".to_string()))?;
        Ok(PromptTemplateVo::from_model(model))
    }

    pub async fn create_prompt_template(
        &self,
        dto: CreatePromptTemplateDto,
        operator: &str,
    ) -> ApiResult<PromptTemplateVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建模板失败")
            .map_err(ApiErrors::Internal)?;
        Ok(PromptTemplateVo::from_model(model))
    }

    pub async fn update_prompt_template(
        &self,
        id: i64,
        dto: UpdatePromptTemplateDto,
        operator: &str,
    ) -> ApiResult<PromptTemplateVo> {
        let model = prompt_template::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询模板失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("模板不存在".to_string()))?;
        let mut active: prompt_template::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        let updated = active
            .update(&self.db)
            .await
            .context("更新模板失败")
            .map_err(ApiErrors::Internal)?;
        Ok(PromptTemplateVo::from_model(updated))
    }

    pub async fn delete_prompt_template(&self, id: i64) -> ApiResult<()> {
        prompt_template::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除模板失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }
}
