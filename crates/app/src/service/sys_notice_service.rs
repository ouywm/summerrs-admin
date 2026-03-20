use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_notice::{CreateNoticeDto, NoticeQueryDto, UpdateNoticeDto};
use model::entity::{
    sys_notice, sys_notice_target, sys_notice_user, sys_role, sys_user, sys_user_role,
};
use model::vo::sys_notice::{NoticeDetailVo, NoticeTargetRoleVo, NoticeTargetUserVo, NoticeVo};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use std::collections::{BTreeSet, HashMap};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysNoticeService {
    #[inject(component)]
    db: DbConn,
}

#[derive(Default)]
struct NoticeTargetIds {
    role_ids: Vec<i64>,
    user_ids: Vec<i64>,
}

#[derive(Default)]
struct NoticeTargetDetails {
    role_ids: Vec<i64>,
    roles: Vec<NoticeTargetRoleVo>,
    user_ids: Vec<i64>,
    users: Vec<NoticeTargetUserVo>,
}

impl SysNoticeService {
    pub async fn list(
        &self,
        query: NoticeQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<NoticeVo>> {
        let page = sys_notice::Entity::find()
            .filter(query)
            .order_by_desc(sys_notice::Column::Pinned)
            .order_by_asc(sys_notice::Column::Sort)
            .order_by_desc(sys_notice::Column::PublishTime)
            .order_by_desc(sys_notice::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询系统公告列表失败")?;

        Ok(page.map(NoticeVo::from))
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<NoticeDetailVo> {
        let notice = Self::find_notice(&self.db, id).await?;
        let targets = Self::load_target_details(&self.db, id).await?;
        Ok(NoticeDetailVo::from_model(
            notice,
            targets.role_ids,
            targets.roles,
            targets.user_ids,
            targets.users,
        ))
    }

    pub async fn create(&self, dto: CreateNoticeDto, operator: &str) -> ApiResult<()> {
        let operator = operator.to_string();
        let scope = dto
            .notice_scope
            .unwrap_or(sys_notice::NoticeScope::AllAdmin);
        let targets = scoped_targets(
            scope,
            normalize_ids(dto.target_role_ids.clone().unwrap_or_default()),
            normalize_ids(dto.target_user_ids.clone().unwrap_or_default()),
        );

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                let role_ids = targets.role_ids.clone();
                let user_ids = targets.user_ids.clone();
                let dto = dto.clone();
                Box::pin(async move {
                    Self::validate_scope_targets(txn, scope, &role_ids, &user_ids).await?;

                    let notice = dto
                        .into_active_model(operator)
                        .insert(txn)
                        .await
                        .context("创建系统公告失败")
                        .map_err(ApiErrors::Internal)?;

                    Self::sync_notice_targets(txn, notice.id, &role_ids, &user_ids).await?;
                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateNoticeDto, operator: &str) -> ApiResult<()> {
        let operator = operator.to_string();
        let next_scope = dto.notice_scope;
        let next_role_ids = dto.target_role_ids.clone();
        let next_user_ids = dto.target_user_ids.clone();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                let dto = dto.clone();
                let next_role_ids = next_role_ids.clone();
                let next_user_ids = next_user_ids.clone();
                Box::pin(async move {
                    let notice = Self::find_notice(txn, id).await?;
                    if notice.publish_status == sys_notice::PublishStatus::Published {
                        return Err(ApiErrors::BadRequest(
                            "已发布公告请先撤回再修改".to_string(),
                        ));
                    }

                    let current_targets = Self::load_target_ids(txn, id).await?;
                    let scope = next_scope.unwrap_or(notice.notice_scope);
                    let targets = scoped_targets(
                        scope,
                        next_role_ids
                            .map(normalize_ids)
                            .unwrap_or(current_targets.role_ids),
                        next_user_ids
                            .map(normalize_ids)
                            .unwrap_or(current_targets.user_ids),
                    );

                    Self::validate_scope_targets(txn, scope, &targets.role_ids, &targets.user_ids)
                        .await?;

                    let mut active: sys_notice::ActiveModel = notice.into();
                    dto.apply_to(&mut active, &operator);
                    active
                        .update(txn)
                        .await
                        .context("更新系统公告失败")
                        .map_err(ApiErrors::Internal)?;

                    Self::sync_notice_targets(txn, id, &targets.role_ids, &targets.user_ids)
                        .await?;
                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                Box::pin(async move {
                    let notice = Self::find_notice(txn, id).await?;
                    if notice.publish_status == sys_notice::PublishStatus::Published {
                        return Err(ApiErrors::BadRequest(
                            "已发布公告请先撤回再删除".to_string(),
                        ));
                    }

                    sys_notice_target::Entity::delete_many()
                        .filter(sys_notice_target::Column::NoticeId.eq(id))
                        .exec(txn)
                        .await
                        .context("删除公告目标失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_notice_user::Entity::delete_many()
                        .filter(sys_notice_user::Column::NoticeId.eq(id))
                        .exec(txn)
                        .await
                        .context("删除公告接收记录失败")
                        .map_err(ApiErrors::Internal)?;

                    sys_notice::Entity::delete_by_id(id)
                        .exec(txn)
                        .await
                        .context("删除系统公告失败")
                        .map_err(ApiErrors::Internal)?;

                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn publish(&self, id: i64, operator: &str) -> ApiResult<()> {
        let operator = operator.to_string();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    let notice = Self::find_notice(txn, id).await?;
                    if notice.publish_status == sys_notice::PublishStatus::Published {
                        return Err(ApiErrors::BadRequest(
                            "公告已发布，无需重复发布".to_string(),
                        ));
                    }

                    let targets = Self::load_target_ids(txn, id).await?;
                    Self::validate_scope_targets(
                        txn,
                        notice.notice_scope,
                        &targets.role_ids,
                        &targets.user_ids,
                    )
                    .await?;

                    let recipient_user_ids = Self::resolve_recipient_user_ids(
                        txn,
                        notice.notice_scope,
                        &targets.role_ids,
                        &targets.user_ids,
                    )
                    .await?;

                    let now = chrono::Local::now().naive_local();
                    let mut active: sys_notice::ActiveModel = notice.into();
                    active.publish_status = Set(sys_notice::PublishStatus::Published);
                    active.publish_by = Set(operator.clone());
                    active.publish_time = Set(Some(now));
                    active.update_by = Set(operator);
                    active
                        .update(txn)
                        .await
                        .context("发布系统公告失败")
                        .map_err(ApiErrors::Internal)?;

                    Self::sync_notice_users(txn, id, &recipient_user_ids).await?;
                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn revoke(&self, id: i64, operator: &str) -> ApiResult<()> {
        let operator = operator.to_string();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                Box::pin(async move {
                    let notice = Self::find_notice(txn, id).await?;
                    if notice.publish_status != sys_notice::PublishStatus::Published {
                        return Err(ApiErrors::BadRequest(
                            "只有已发布公告才可以撤回".to_string(),
                        ));
                    }

                    let mut active: sys_notice::ActiveModel = notice.into();
                    active.publish_status = Set(sys_notice::PublishStatus::Revoked);
                    active.update_by = Set(operator);
                    active
                        .update(txn)
                        .await
                        .context("撤回系统公告失败")
                        .map_err(ApiErrors::Internal)?;

                    Ok(())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn pin(&self, id: i64, operator: &str) -> ApiResult<()> {
        self.set_pinned(id, true, operator).await
    }

    pub async fn unpin(&self, id: i64, operator: &str) -> ApiResult<()> {
        self.set_pinned(id, false, operator).await
    }

    async fn set_pinned(&self, id: i64, pinned: bool, operator: &str) -> ApiResult<()> {
        let notice = Self::find_notice(&self.db, id).await?;
        let mut active: sys_notice::ActiveModel = notice.into();
        active.pinned = Set(pinned);
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.with_context(|| {
            if pinned {
                "设置公告置顶失败"
            } else {
                "取消公告置顶失败"
            }
        })?;
        Ok(())
    }

    async fn find_notice<C>(db: &C, id: i64) -> ApiResult<sys_notice::Model>
    where
        C: ConnectionTrait,
    {
        sys_notice::Entity::find_by_id(id)
            .one(db)
            .await
            .context("查询系统公告详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统公告不存在".to_string()))
    }

    async fn load_target_ids<C>(db: &C, notice_id: i64) -> ApiResult<NoticeTargetIds>
    where
        C: ConnectionTrait,
    {
        let rows = sys_notice_target::Entity::find()
            .filter(sys_notice_target::Column::NoticeId.eq(notice_id))
            .order_by_asc(sys_notice_target::Column::TargetType)
            .order_by_asc(sys_notice_target::Column::TargetId)
            .all(db)
            .await
            .context("查询公告目标失败")?;

        let mut result = NoticeTargetIds::default();
        for row in rows {
            match row.target_type {
                sys_notice_target::NoticeTargetType::Role => result.role_ids.push(row.target_id),
                sys_notice_target::NoticeTargetType::User => result.user_ids.push(row.target_id),
            }
        }
        Ok(result)
    }

    async fn load_target_details<C>(db: &C, notice_id: i64) -> ApiResult<NoticeTargetDetails>
    where
        C: ConnectionTrait,
    {
        let target_ids = Self::load_target_ids(db, notice_id).await?;

        let role_models = if target_ids.role_ids.is_empty() {
            Vec::new()
        } else {
            sys_role::Entity::find()
                .filter(sys_role::Column::Id.is_in(target_ids.role_ids.iter().copied()))
                .all(db)
                .await
                .context("查询公告目标角色失败")?
        };
        let role_map: HashMap<i64, sys_role::Model> = role_models
            .into_iter()
            .map(|model| (model.id, model))
            .collect();
        let roles = target_ids
            .role_ids
            .iter()
            .filter_map(|id| role_map.get(id).cloned().map(NoticeTargetRoleVo::from))
            .collect();

        let user_models = if target_ids.user_ids.is_empty() {
            Vec::new()
        } else {
            sys_user::Entity::find()
                .filter(sys_user::Column::Id.is_in(target_ids.user_ids.iter().copied()))
                .all(db)
                .await
                .context("查询公告目标用户失败")?
        };
        let user_map: HashMap<i64, sys_user::Model> = user_models
            .into_iter()
            .map(|model| (model.id, model))
            .collect();
        let users = target_ids
            .user_ids
            .iter()
            .filter_map(|id| user_map.get(id).cloned().map(NoticeTargetUserVo::from))
            .collect();

        Ok(NoticeTargetDetails {
            role_ids: target_ids.role_ids,
            roles,
            user_ids: target_ids.user_ids,
            users,
        })
    }

    async fn validate_scope_targets<C>(
        db: &C,
        scope: sys_notice::NoticeScope,
        role_ids: &[i64],
        user_ids: &[i64],
    ) -> ApiResult<()>
    where
        C: ConnectionTrait,
    {
        match scope {
            sys_notice::NoticeScope::AllAdmin => Ok(()),
            sys_notice::NoticeScope::Role => {
                if role_ids.is_empty() {
                    return Err(ApiErrors::BadRequest(
                        "指定角色公告必须选择角色".to_string(),
                    ));
                }
                let count = sys_role::Entity::find()
                    .filter(sys_role::Column::Id.is_in(role_ids.iter().copied()))
                    .count(db)
                    .await
                    .context("校验公告角色失败")?;
                if count != role_ids.len() as u64 {
                    return Err(ApiErrors::BadRequest("存在无效的角色ID".to_string()));
                }
                Ok(())
            }
            sys_notice::NoticeScope::User => {
                if user_ids.is_empty() {
                    return Err(ApiErrors::BadRequest(
                        "指定用户公告必须选择用户".to_string(),
                    ));
                }
                let count = sys_user::Entity::find()
                    .filter(sys_user::Column::Id.is_in(user_ids.iter().copied()))
                    .count(db)
                    .await
                    .context("校验公告用户失败")?;
                if count != user_ids.len() as u64 {
                    return Err(ApiErrors::BadRequest("存在无效的用户ID".to_string()));
                }
                Ok(())
            }
        }
    }

    async fn sync_notice_targets<C>(
        db: &C,
        notice_id: i64,
        role_ids: &[i64],
        user_ids: &[i64],
    ) -> ApiResult<()>
    where
        C: ConnectionTrait,
    {
        sys_notice_target::Entity::delete_many()
            .filter(sys_notice_target::Column::NoticeId.eq(notice_id))
            .exec(db)
            .await
            .context("清理公告目标失败")?;

        let models: Vec<sys_notice_target::ActiveModel> = role_ids
            .iter()
            .copied()
            .map(|target_id| sys_notice_target::ActiveModel {
                notice_id: Set(notice_id),
                target_type: Set(sys_notice_target::NoticeTargetType::Role),
                target_id: Set(target_id),
                ..Default::default()
            })
            .chain(
                user_ids
                    .iter()
                    .copied()
                    .map(|target_id| sys_notice_target::ActiveModel {
                        notice_id: Set(notice_id),
                        target_type: Set(sys_notice_target::NoticeTargetType::User),
                        target_id: Set(target_id),
                        ..Default::default()
                    }),
            )
            .collect();

        if !models.is_empty() {
            sys_notice_target::Entity::insert_many(models)
                .exec(db)
                .await
                .context("保存公告目标失败")?;
        }

        Ok(())
    }

    async fn resolve_recipient_user_ids<C>(
        db: &C,
        scope: sys_notice::NoticeScope,
        role_ids: &[i64],
        user_ids: &[i64],
    ) -> ApiResult<Vec<i64>>
    where
        C: ConnectionTrait,
    {
        match scope {
            sys_notice::NoticeScope::AllAdmin => {
                let ids = sys_user::Entity::find()
                    .all(db)
                    .await
                    .context("查询公告接收用户失败")?
                    .into_iter()
                    .map(|user| user.id)
                    .collect();
                Ok(ids)
            }
            sys_notice::NoticeScope::Role => {
                let related_user_ids: Vec<i64> = sys_user_role::Entity::find()
                    .filter(sys_user_role::Column::RoleId.is_in(role_ids.iter().copied()))
                    .all(db)
                    .await
                    .context("查询角色用户失败")?
                    .into_iter()
                    .map(|item| item.user_id)
                    .collect();

                Self::filter_existing_user_ids(db, related_user_ids).await
            }
            sys_notice::NoticeScope::User => {
                Self::filter_existing_user_ids(db, user_ids.to_vec()).await
            }
        }
    }

    async fn filter_existing_user_ids<C>(db: &C, user_ids: Vec<i64>) -> ApiResult<Vec<i64>>
    where
        C: ConnectionTrait,
    {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let unique_ids = normalize_ids(user_ids);
        let ids = sys_user::Entity::find()
            .filter(sys_user::Column::Id.is_in(unique_ids.iter().copied()))
            .all(db)
            .await
            .context("过滤公告接收用户失败")?
            .into_iter()
            .map(|user| user.id)
            .collect();
        Ok(ids)
    }

    async fn sync_notice_users<C>(db: &C, notice_id: i64, user_ids: &[i64]) -> ApiResult<()>
    where
        C: ConnectionTrait,
    {
        sys_notice_user::Entity::delete_many()
            .filter(sys_notice_user::Column::NoticeId.eq(notice_id))
            .exec(db)
            .await
            .context("清理公告接收记录失败")?;

        if user_ids.is_empty() {
            return Ok(());
        }

        let models: Vec<sys_notice_user::ActiveModel> = user_ids
            .iter()
            .copied()
            .map(|user_id| sys_notice_user::ActiveModel {
                notice_id: Set(notice_id),
                user_id: Set(user_id),
                read_flag: Set(false),
                read_time: Set(None),
                ..Default::default()
            })
            .collect();

        sys_notice_user::Entity::insert_many(models)
            .exec(db)
            .await
            .context("生成公告接收记录失败")?;

        Ok(())
    }
}

fn normalize_ids(ids: Vec<i64>) -> Vec<i64> {
    BTreeSet::from_iter(ids.into_iter().filter(|id| *id > 0))
        .into_iter()
        .collect()
}

fn scoped_targets(
    scope: sys_notice::NoticeScope,
    role_ids: Vec<i64>,
    user_ids: Vec<i64>,
) -> NoticeTargetIds {
    match scope {
        sys_notice::NoticeScope::AllAdmin => NoticeTargetIds::default(),
        sys_notice::NoticeScope::Role => NoticeTargetIds {
            role_ids,
            user_ids: Vec::new(),
        },
        sys_notice::NoticeScope::User => NoticeTargetIds {
            role_ids: Vec::new(),
            user_ids,
        },
    }
}
