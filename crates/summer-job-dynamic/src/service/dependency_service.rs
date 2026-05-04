use std::collections::HashSet;

use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use crate::dto::{DependencyVo, JobDependencyListVo};
use crate::entity::{sys_job, sys_job_dependency};
use crate::enums::DependencyOnState;

const MAX_CYCLE_DETECT_HOPS: usize = 100;

#[derive(Clone, Service)]
pub struct DependencyService {
    #[inject(component)]
    db: DbConn,
}

impl DependencyService {
    pub async fn add(
        &self,
        upstream_id: i64,
        downstream_id: i64,
        on_state: DependencyOnState,
    ) -> ApiResult<sys_job_dependency::Model> {
        if upstream_id == downstream_id {
            return Err(ApiErrors::BadRequest(
                "不允许 upstream 与 downstream 相同".into(),
            ));
        }

        let exists = sys_job::Entity::find()
            .filter(sys_job::Column::Id.is_in([upstream_id, downstream_id]))
            .all(&self.db)
            .await
            .context("校验任务存在性失败")?;
        if exists.len() != 2 {
            return Err(ApiErrors::BadRequest(format!(
                "upstream {upstream_id} 或 downstream {downstream_id} 不存在"
            )));
        }

        let dup = sys_job_dependency::Entity::find()
            .filter(sys_job_dependency::Column::UpstreamId.eq(upstream_id))
            .filter(sys_job_dependency::Column::DownstreamId.eq(downstream_id))
            .one(&self.db)
            .await
            .context("查询依赖重复性失败")?;
        if dup.is_some() {
            return Err(ApiErrors::BadRequest("依赖关系已存在".into()));
        }

        if self.would_form_cycle(upstream_id, downstream_id).await? {
            return Err(ApiErrors::BadRequest(format!(
                "添加 {upstream_id} -> {downstream_id} 会形成依赖环"
            )));
        }

        let active = sys_job_dependency::ActiveModel {
            upstream_id: Set(upstream_id),
            downstream_id: Set(downstream_id),
            on_state: Set(on_state),
            enabled: Set(true),
            ..Default::default()
        };
        let model = active.insert(&self.db).await.context("插入依赖失败")?;
        Ok(model)
    }

    pub async fn remove(&self, id: i64) -> ApiResult<()> {
        let res = sys_job_dependency::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除依赖失败")?;
        if res.rows_affected == 0 {
            return Err(ApiErrors::NotFound(format!("依赖不存在: {id}")));
        }
        Ok(())
    }

    pub async fn list_for_job(&self, job_id: i64) -> ApiResult<JobDependencyListVo> {
        let outgoing_rows = sys_job_dependency::Entity::find()
            .filter(sys_job_dependency::Column::UpstreamId.eq(job_id))
            .order_by_asc(sys_job_dependency::Column::Id)
            .all(&self.db)
            .await
            .context("查询出向依赖失败")?;
        let incoming_rows = sys_job_dependency::Entity::find()
            .filter(sys_job_dependency::Column::DownstreamId.eq(job_id))
            .order_by_asc(sys_job_dependency::Column::Id)
            .all(&self.db)
            .await
            .context("查询入向依赖失败")?;

        let mut name_ids: HashSet<i64> = HashSet::new();
        for r in outgoing_rows.iter().chain(incoming_rows.iter()) {
            name_ids.insert(r.upstream_id);
            name_ids.insert(r.downstream_id);
        }
        let names = self.fetch_job_names(name_ids.into_iter().collect()).await?;
        let resolve = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("#{id}"));

        let outgoing = outgoing_rows
            .into_iter()
            .map(|r| DependencyVo {
                id: r.id,
                upstream_id: r.upstream_id,
                upstream_name: resolve(r.upstream_id),
                downstream_id: r.downstream_id,
                downstream_name: resolve(r.downstream_id),
                on_state: r.on_state,
                enabled: r.enabled,
                create_time: r.create_time,
            })
            .collect();
        let incoming = incoming_rows
            .into_iter()
            .map(|r| DependencyVo {
                id: r.id,
                upstream_id: r.upstream_id,
                upstream_name: resolve(r.upstream_id),
                downstream_id: r.downstream_id,
                downstream_name: resolve(r.downstream_id),
                on_state: r.on_state,
                enabled: r.enabled,
                create_time: r.create_time,
            })
            .collect();
        Ok(JobDependencyListVo { outgoing, incoming })
    }

    /// 给 worker 钩子用：给定 upstream 跑完的终态，返回应该被触发的下游 job_id 列表。
    /// 失败只 log，不返回 Err（依赖触发是 best-effort，绝不阻塞 worker）。
    pub async fn list_to_fire(
        &self,
        upstream_id: i64,
        upstream_terminal: DependencyOnState,
    ) -> Vec<i64> {
        let rows = match sys_job_dependency::Entity::find()
            .filter(sys_job_dependency::Column::UpstreamId.eq(upstream_id))
            .filter(sys_job_dependency::Column::Enabled.eq(true))
            .all(&self.db)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(?error, upstream_id, "list dependency for fire failed");
                return Vec::new();
            }
        };

        rows.into_iter()
            .filter(|r| match (r.on_state, upstream_terminal) {
                (DependencyOnState::Always, _) => true,
                (a, b) => a == b,
            })
            .map(|r| r.downstream_id)
            .collect()
    }

    async fn fetch_job_names(
        &self,
        ids: Vec<i64>,
    ) -> ApiResult<std::collections::HashMap<i64, String>> {
        if ids.is_empty() {
            return Ok(Default::default());
        }
        let rows = sys_job::Entity::find()
            .filter(sys_job::Column::Id.is_in(ids))
            .all(&self.db)
            .await
            .context("查询任务名称失败")?;
        Ok(rows.into_iter().map(|j| (j.id, j.name)).collect())
    }

    /// BFS：从 downstream 出发沿 upstream→downstream 边走，看能不能回到 upstream。
    /// 走到则形成环。最多走 MAX_CYCLE_DETECT_HOPS 跳就停（防数据腐坏导致死循环）。
    async fn would_form_cycle(&self, upstream: i64, downstream: i64) -> ApiResult<bool> {
        let mut visited: HashSet<i64> = HashSet::new();
        let mut frontier: Vec<i64> = vec![downstream];
        let mut hops = 0usize;
        while let Some(node) = frontier.pop() {
            hops += 1;
            if hops > MAX_CYCLE_DETECT_HOPS {
                return Err(ApiErrors::BadRequest("依赖图过深，疑似环".into()));
            }
            if !visited.insert(node) {
                continue;
            }
            if node == upstream {
                return Ok(true);
            }
            let next = sys_job_dependency::Entity::find()
                .filter(sys_job_dependency::Column::UpstreamId.eq(node))
                .all(&self.db)
                .await
                .context("查询下游依赖失败")?;
            for n in next {
                frontier.push(n.downstream_id);
            }
        }
        Ok(false)
    }
}
