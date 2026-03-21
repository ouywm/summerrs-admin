use anyhow::Context;
use summer_common::error::{ApiErrors, ApiResult};
use summer_model::dto::sys_notice::{UserNoticeLatestQueryDto, UserNoticeQueryDto};
use summer_model::entity::{sys_notice, sys_notice_user};
use summer_model::vo::sys_notice::{NoticeUnreadCountVo, UserNoticeDetailVo, UserNoticeVo};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};
use std::collections::HashMap;
use summer::plugin::Service;
use summer_auth::LoginId;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct UserNoticeService {
    #[inject(component)]
    db: DbConn,
}

impl UserNoticeService {
    pub async fn list(
        &self,
        login_id: &LoginId,
        query: UserNoticeQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<UserNoticeVo>> {
        let page = self
            .visible_notice_query(login_id.user_id)
            .filter(query)
            .order_by_desc(sys_notice::Column::Pinned)
            .order_by_asc(sys_notice::Column::Sort)
            .order_by_desc(sys_notice::Column::PublishTime)
            .order_by_desc(sys_notice::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询用户公告列表失败")?;

        if page.content.is_empty() {
            return Ok(pagination.empty_page());
        }

        let total = page.total_elements;
        let content = self.build_user_notice_vos(page.content).await?;
        Ok(Page::new(content, &pagination, total))
    }

    pub async fn latest(
        &self,
        login_id: &LoginId,
        query: UserNoticeLatestQueryDto,
    ) -> ApiResult<Vec<UserNoticeVo>> {
        let mut select = self
            .visible_notice_query(login_id.user_id)
            .order_by_desc(sys_notice::Column::Pinned)
            .order_by_asc(sys_notice::Column::Sort)
            .order_by_desc(sys_notice::Column::PublishTime)
            .order_by_desc(sys_notice::Column::Id);
        if let Some(read_flag) = query.read_flag {
            select = select.filter(sys_notice_user::Column::ReadFlag.eq(read_flag));
        }

        let rows = select
            .limit(query.limit())
            .all(&self.db)
            .await
            .context("查询最新公告失败")?;
        self.build_user_notice_vos(rows).await
    }

    pub async fn unread_count(&self, login_id: &LoginId) -> ApiResult<NoticeUnreadCountVo> {
        let unread_count = self
            .visible_notice_query(login_id.user_id)
            .filter(sys_notice_user::Column::ReadFlag.eq(false))
            .count(&self.db)
            .await
            .context("查询未读公告数量失败")?;

        Ok(NoticeUnreadCountVo { unread_count })
    }

    pub async fn detail(
        &self,
        login_id: &LoginId,
        notice_id: i64,
    ) -> ApiResult<UserNoticeDetailVo> {
        let notice_user = self
            .visible_notice_query(login_id.user_id)
            .filter(sys_notice_user::Column::NoticeId.eq(notice_id))
            .one(&self.db)
            .await
            .context("查询公告详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("公告不存在或无权查看".to_string()))?;

        let notice = self
            .load_notice_map(vec![notice_id])
            .await?
            .remove(&notice_id)
            .ok_or_else(|| ApiErrors::NotFound("公告不存在或无权查看".to_string()))?;

        Ok(UserNoticeDetailVo::from_models(notice, notice_user))
    }

    pub async fn read(&self, login_id: &LoginId, notice_id: i64) -> ApiResult<()> {
        let notice_user = self
            .visible_notice_query(login_id.user_id)
            .filter(sys_notice_user::Column::NoticeId.eq(notice_id))
            .one(&self.db)
            .await
            .context("查询公告已读状态失败")?
            .ok_or_else(|| ApiErrors::NotFound("公告不存在或无权查看".to_string()))?;

        if notice_user.read_flag {
            return Ok(());
        }

        let now = chrono::Local::now().naive_local();
        let mut active: sys_notice_user::ActiveModel = notice_user.into();
        active.read_flag = Set(true);
        active.read_time = Set(Some(now));
        active.update(&self.db).await.context("标记公告已读失败")?;
        Ok(())
    }

    pub async fn read_all(&self, login_id: &LoginId) -> ApiResult<()> {
        let ids: Vec<i64> = self
            .visible_notice_query(login_id.user_id)
            .select_only()
            .column(sys_notice_user::Column::Id)
            .filter(sys_notice_user::Column::ReadFlag.eq(false))
            .into_tuple()
            .all(&self.db)
            .await
            .context("查询未读公告失败")?;
        if ids.is_empty() {
            return Ok(());
        }

        let now = chrono::Local::now().naive_local();
        sys_notice_user::Entity::update_many()
            .set(sys_notice_user::ActiveModel {
                read_flag: Set(true),
                read_time: Set(Some(now)),
                ..Default::default()
            })
            .filter(sys_notice_user::Column::Id.is_in(ids))
            .exec(&self.db)
            .await
            .context("批量标记公告已读失败")?;

        Ok(())
    }

    fn visible_notice_query(&self, user_id: i64) -> sea_orm::Select<sys_notice_user::Entity> {
        sys_notice_user::Entity::find()
            .join(
                JoinType::InnerJoin,
                sys_notice_user::Relation::SysNotice.def(),
            )
            .filter(self.visible_notice_condition(user_id))
    }

    fn visible_notice_condition(&self, user_id: i64) -> sea_orm::Condition {
        let now = chrono::Local::now().naive_local();
        sea_orm::Condition::all()
            .add(sys_notice_user::Column::UserId.eq(user_id))
            .add(sys_notice::Column::PublishStatus.eq(sys_notice::PublishStatus::Published))
            .add(sys_notice::Column::Enabled.eq(true))
            .add(
                sea_orm::Condition::any()
                    .add(sys_notice::Column::ExpireTime.is_null())
                    .add(sys_notice::Column::ExpireTime.gte(now)),
            )
    }

    async fn build_user_notice_vos(
        &self,
        rows: Vec<sys_notice_user::Model>,
    ) -> ApiResult<Vec<UserNoticeVo>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let notice_ids: Vec<i64> = rows.iter().map(|row| row.notice_id).collect();
        let notice_map = self.load_notice_map(notice_ids).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                notice_map
                    .get(&row.notice_id)
                    .cloned()
                    .map(|notice| UserNoticeVo::from_models(notice, row))
            })
            .collect())
    }

    async fn load_notice_map(
        &self,
        notice_ids: Vec<i64>,
    ) -> ApiResult<HashMap<i64, sys_notice::Model>> {
        if notice_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let notices = sys_notice::Entity::find()
            .filter(sys_notice::Column::Id.is_in(notice_ids))
            .all(&self.db)
            .await
            .context("查询公告信息失败")?;

        Ok(notices
            .into_iter()
            .map(|notice| (notice.id, notice))
            .collect())
    }
}
