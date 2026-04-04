use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::conversation::ConversationService;
use summer_ai_model::dto::conversation::{
    CreateConversationDto, CreateMessageDto, CreatePromptTemplateDto, QueryConversationDto,
    QueryMessageDto, QueryPromptTemplateDto, UpdateConversationDto, UpdatePromptTemplateDto,
};
use summer_ai_model::vo::conversation::{
    ConversationDetailVo, ConversationVo, MessageVo, PromptTemplateVo,
};

// ─── Conversation ───

#[get_api("/ai/conversation")]
pub async fn list_conversations(
    Component(svc): Component<ConversationService>,
    Query(query): Query<QueryConversationDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ConversationVo>>> {
    Ok(Json(svc.list_conversations(query, pagination).await?))
}

#[get_api("/ai/conversation/{id}")]
pub async fn get_conversation(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConversationDetailVo>> {
    Ok(Json(svc.get_conversation(id).await?))
}

#[post_api("/ai/conversation")]
pub async fn create_conversation(
    Component(svc): Component<ConversationService>,
    ValidatedJson(dto): ValidatedJson<CreateConversationDto>,
) -> ApiResult<Json<ConversationVo>> {
    Ok(Json(svc.create_conversation(dto).await?))
}

#[put_api("/ai/conversation/{id}")]
pub async fn update_conversation(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConversationDto>,
) -> ApiResult<Json<ConversationVo>> {
    Ok(Json(svc.update_conversation(id, dto).await?))
}

#[delete_api("/ai/conversation/{id}")]
pub async fn delete_conversation(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_conversation(id).await
}

// ─── Message ───

#[get_api("/ai/message")]
pub async fn list_messages(
    Component(svc): Component<ConversationService>,
    Query(query): Query<QueryMessageDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<MessageVo>>> {
    Ok(Json(svc.list_messages(query, pagination).await?))
}

#[post_api("/ai/message")]
pub async fn create_message(
    Component(svc): Component<ConversationService>,
    ValidatedJson(dto): ValidatedJson<CreateMessageDto>,
) -> ApiResult<Json<MessageVo>> {
    Ok(Json(svc.create_message(dto).await?))
}

#[delete_api("/ai/message/{id}")]
pub async fn delete_message(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_message(id).await
}

// ─── PromptTemplate ───

#[get_api("/ai/prompt-template")]
pub async fn list_prompt_templates(
    Component(svc): Component<ConversationService>,
    Query(query): Query<QueryPromptTemplateDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<PromptTemplateVo>>> {
    Ok(Json(svc.list_prompt_templates(query, pagination).await?))
}

#[get_api("/ai/prompt-template/{id}")]
pub async fn get_prompt_template(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PromptTemplateVo>> {
    Ok(Json(svc.get_prompt_template(id).await?))
}

#[post_api("/ai/prompt-template")]
pub async fn create_prompt_template(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ConversationService>,
    ValidatedJson(dto): ValidatedJson<CreatePromptTemplateDto>,
) -> ApiResult<Json<PromptTemplateVo>> {
    Ok(Json(
        svc.create_prompt_template(dto, &profile.nick_name).await?,
    ))
}

#[put_api("/ai/prompt-template/{id}")]
pub async fn update_prompt_template(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdatePromptTemplateDto>,
) -> ApiResult<Json<PromptTemplateVo>> {
    Ok(Json(
        svc.update_prompt_template(id, dto, &profile.nick_name)
            .await?,
    ))
}

#[delete_api("/ai/prompt-template/{id}")]
pub async fn delete_prompt_template(
    Component(svc): Component<ConversationService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_prompt_template(id).await
}
