use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_sharding::{
    DataSourceHealth, DataSourceRouteState, ShardingConnection,
    TenantIsolationLevel as RuntimeTenantIsolationLevel, TenantLifecycleManager,
    TenantMetadataRecord,
};
use summer_system_model::dto::sys_tenant::{
    ChangeTenantStatusDto, CreateTenantDto, ProvisionTenantDto, SaveTenantDatasourceDto,
    SaveTenantMembershipDto, TenantQueryDto, UpdateTenantDto,
};
use summer_system_model::entity::{
    sys_tenant, sys_tenant_datasource, sys_tenant_membership, sys_user,
};
use summer_system_model::vo::sys_tenant::{
    TenantDatasourceVo, TenantDetailVo, TenantMembershipVo, TenantProvisionResultVo,
    TenantRouteStateVo, TenantRuntimeDatasourceVo, TenantRuntimeRefreshVo, TenantVo,
};

#[derive(Clone, Service)]
pub struct SysTenantService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    sharding: ShardingConnection,
}

impl SysTenantService {
    pub async fn list_tenants(
        &self,
        query: TenantQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TenantVo>> {
        let page = sys_tenant::Entity::find()
            .filter(query)
            .order_by_desc(sys_tenant::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询租户列表失败")?;

        if page.is_empty() {
            return Ok(page.map(|model| TenantVo::from_model(model, None, 0)));
        }

        let tenant_ids = page
            .content
            .iter()
            .map(|model| model.tenant_id.clone())
            .collect::<Vec<_>>();
        let datasource_map = self.load_datasource_map(&tenant_ids).await?;
        let membership_counts = self.load_membership_count_map(&tenant_ids).await?;

        Ok(page.map(|model| {
            let tenant_id = model.tenant_id.clone();
            let datasource = datasource_map.get(&tenant_id).cloned();
            let member_count = membership_counts
                .get(&tenant_id)
                .copied()
                .unwrap_or_default();
            TenantVo::from_model(model, datasource, member_count)
        }))
    }

    pub async fn get_tenant_detail(&self, tenant_id: &str) -> ApiResult<TenantDetailVo> {
        let tenant = self.find_tenant_model(tenant_id).await?;
        let datasource = self.find_tenant_datasource_model(tenant_id).await?;
        let member_count = self
            .load_membership_count_map(&[tenant_id.to_string()])
            .await?
            .get(tenant_id)
            .copied()
            .unwrap_or_default();

        Ok(TenantDetailVo {
            config: tenant.config.clone(),
            metadata: tenant.metadata.clone(),
            tenant: TenantVo::from_model(tenant, datasource, member_count),
        })
    }

    pub async fn create_tenant(&self, dto: CreateTenantDto, operator: &str) -> ApiResult<()> {
        let existing = sys_tenant::Entity::find()
            .filter(sys_tenant::Column::TenantId.eq(dto.tenant_id.clone()))
            .one(&self.db)
            .await
            .context("检查租户标识失败")?;
        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "租户标识已存在: {}",
                dto.tenant_id
            )));
        }

        let default_isolation_level = dto
            .default_isolation_level
            .unwrap_or(sys_tenant::TenantIsolationLevel::SharedRow);
        let tenant_status = dto.status.unwrap_or(sys_tenant::TenantStatus::Enabled);
        let tenant_id = dto.tenant_id.clone();
        let operator = operator.to_string();
        let db = self.db.clone();
        db.transaction::<_, (), ApiErrors>(move |txn| {
            let dto = dto.clone();
            let operator = operator.clone();
            let tenant_id = tenant_id.clone();
            Box::pin(async move {
                dto.into_active_model(operator.clone())
                    .insert(txn)
                    .await
                    .context("创建租户失败")
                    .map_err(ApiErrors::Internal)?;

                sys_tenant_datasource::ActiveModel {
                    tenant_id: Set(tenant_id),
                    isolation_level: Set(tenant_to_datasource_isolation(default_isolation_level)),
                    status: Set(datasource_status_for_tenant_status(tenant_status)),
                    readonly_config: Set(serde_json::json!({})),
                    extra_config: Set(serde_json::json!({})),
                    remark: Set(String::new()),
                    create_by: Set(operator.clone()),
                    update_by: Set(operator),
                    ..Default::default()
                }
                .insert(txn)
                .await
                .context("初始化租户数据源元数据失败")
                .map_err(ApiErrors::Internal)?;

                Ok(())
            })
        })
        .await?;
        let _ = self.refresh_runtime_metadata().await?;
        Ok(())
    }

    pub async fn update_tenant(
        &self,
        tenant_id: &str,
        dto: UpdateTenantDto,
        operator: &str,
    ) -> ApiResult<()> {
        let tenant = self.find_tenant_model(tenant_id).await?;
        let mut active: sys_tenant::ActiveModel = tenant.into();
        dto.apply_to(&mut active, operator);
        active.update(&self.db).await.context("更新租户失败")?;
        Ok(())
    }

    pub async fn change_tenant_status(
        &self,
        tenant_id: &str,
        dto: ChangeTenantStatusDto,
        operator: &str,
    ) -> ApiResult<()> {
        let datasource_status = dto
            .datasource_status
            .unwrap_or_else(|| datasource_status_for_tenant_status(dto.status));

        let db = self.db.clone();
        let tenant_id = tenant_id.to_string();
        let operator = operator.to_string();
        db.transaction::<_, (), ApiErrors>(move |txn| {
            let tenant_id = tenant_id.clone();
            let operator = operator.clone();
            let datasource_status = datasource_status.clone();
            Box::pin(async move {
                let tenant = sys_tenant::Entity::find()
                    .filter(sys_tenant::Column::TenantId.eq(tenant_id.clone()))
                    .one(txn)
                    .await
                    .context("查询租户失败")
                    .map_err(ApiErrors::Internal)?
                    .ok_or_else(|| ApiErrors::NotFound(format!("租户不存在: {tenant_id}")))?;
                let default_isolation_level = tenant.default_isolation_level;

                let mut tenant_active: sys_tenant::ActiveModel = tenant.into();
                tenant_active.status = Set(dto.status);
                tenant_active.update_by = Set(operator.clone());
                tenant_active
                    .update(txn)
                    .await
                    .context("更新租户状态失败")
                    .map_err(ApiErrors::Internal)?;

                if let Some(datasource) = sys_tenant_datasource::Entity::find()
                    .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id.clone()))
                    .one(txn)
                    .await
                    .context("查询租户数据源失败")
                    .map_err(ApiErrors::Internal)?
                {
                    let mut datasource_active: sys_tenant_datasource::ActiveModel =
                        datasource.into();
                    datasource_active.status = Set(datasource_status.clone());
                    datasource_active.update_by = Set(operator.clone());
                    datasource_active
                        .update(txn)
                        .await
                        .context("更新租户数据源状态失败")
                        .map_err(ApiErrors::Internal)?;
                } else {
                    sys_tenant_datasource::ActiveModel {
                        tenant_id: Set(tenant_id.clone()),
                        isolation_level: Set(tenant_to_datasource_isolation(
                            default_isolation_level,
                        )),
                        status: Set(datasource_status.clone()),
                        readonly_config: Set(serde_json::json!({})),
                        extra_config: Set(serde_json::json!({})),
                        remark: Set(String::new()),
                        create_by: Set(operator.clone()),
                        update_by: Set(operator.clone()),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await
                    .context("补齐租户数据源状态失败")
                    .map_err(ApiErrors::Internal)?;
                }

                Ok(())
            })
        })
        .await?;

        let _ = self.refresh_runtime_metadata().await?;
        Ok(())
    }

    pub async fn save_tenant_datasource(
        &self,
        tenant_id: &str,
        mut dto: SaveTenantDatasourceDto,
        operator: &str,
    ) -> ApiResult<TenantDatasourceVo> {
        let isolation = dto.isolation_level;
        let tenant = self.find_tenant_model(tenant_id).await?;
        dto.status = Some(
            dto.status
                .unwrap_or_else(|| datasource_status_for_tenant_status(tenant.status)),
        );

        let tenant_id_string = tenant_id.to_string();
        let operator_string = operator.to_string();
        let db = self.db.clone();
        db.transaction::<_, (), ApiErrors>(move |txn| {
            let tenant_id = tenant_id_string.clone();
            let operator = operator_string.clone();
            let dto = dto.clone();
            Box::pin(async move {
                if let Some(datasource) = sys_tenant_datasource::Entity::find()
                    .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id.clone()))
                    .one(txn)
                    .await
                    .context("查询租户数据源失败")
                    .map_err(ApiErrors::Internal)?
                {
                    let mut active: sys_tenant_datasource::ActiveModel = datasource.into();
                    dto.clone().apply_to(&mut active, &operator);
                    active.last_sync_time = Set(Some(now()));
                    active
                        .update(txn)
                        .await
                        .context("更新租户数据源失败")
                        .map_err(ApiErrors::Internal)?;
                } else {
                    let mut active = dto
                        .clone()
                        .into_active_model(tenant_id.clone(), operator.clone());
                    active.last_sync_time = Set(Some(now()));
                    active
                        .insert(txn)
                        .await
                        .context("创建租户数据源失败")
                        .map_err(ApiErrors::Internal)?;
                }

                if let Some(tenant) = sys_tenant::Entity::find()
                    .filter(sys_tenant::Column::TenantId.eq(tenant_id.clone()))
                    .one(txn)
                    .await
                    .context("查询租户失败")
                    .map_err(ApiErrors::Internal)?
                {
                    let mut active: sys_tenant::ActiveModel = tenant.into();
                    active.default_isolation_level = Set(datasource_to_tenant_isolation(isolation));
                    active.update_by = Set(operator.clone());
                    active
                        .update(txn)
                        .await
                        .context("同步租户默认隔离级别失败")
                        .map_err(ApiErrors::Internal)?;
                }

                Ok(())
            })
        })
        .await?;

        let _ = self.refresh_runtime_metadata().await?;
        let datasource = self
            .find_tenant_datasource_model(tenant_id)
            .await?
            .ok_or_else(|| ApiErrors::NotFound(format!("租户数据源不存在: {tenant_id}")))?;
        Ok(TenantDatasourceVo::from(datasource))
    }

    pub async fn list_tenant_members(&self, tenant_id: &str) -> ApiResult<Vec<TenantMembershipVo>> {
        self.find_tenant_model(tenant_id).await?;

        let memberships = sys_tenant_membership::Entity::find()
            .filter(sys_tenant_membership::Column::TenantId.eq(tenant_id))
            .order_by_desc(sys_tenant_membership::Column::IsDefault)
            .order_by_asc(sys_tenant_membership::Column::Id)
            .all(&self.db)
            .await
            .context("查询租户成员失败")?;
        let user_ids = memberships
            .iter()
            .map(|item| item.user_id)
            .collect::<Vec<_>>();
        let user_map = self.load_user_map(&user_ids).await?;

        Ok(memberships
            .into_iter()
            .map(|membership| {
                let user = user_map.get(&membership.user_id);
                TenantMembershipVo {
                    id: membership.id,
                    tenant_id: membership.tenant_id,
                    user_id: membership.user_id,
                    user_name: user
                        .map(|value| value.user_name.clone())
                        .unwrap_or_default(),
                    nick_name: user
                        .map(|value| value.nick_name.clone())
                        .unwrap_or_default(),
                    email: user.map(|value| value.email.clone()).unwrap_or_default(),
                    role_code: membership.role_code,
                    is_default: membership.is_default,
                    status: membership.status,
                    source: membership.source,
                    joined_time: membership.joined_time,
                    last_access_time: membership.last_access_time,
                    remark: membership.remark,
                }
            })
            .collect())
    }

    pub async fn save_tenant_membership(
        &self,
        tenant_id: &str,
        dto: SaveTenantMembershipDto,
        operator: &str,
    ) -> ApiResult<()> {
        self.find_tenant_model(tenant_id).await?;
        sys_user::Entity::find_by_id(dto.user_id)
            .one(&self.db)
            .await
            .context("查询租户成员用户失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("用户不存在: {}", dto.user_id)))?;

        let should_be_default = dto.is_default.unwrap_or(false);
        let effective_status = dto
            .status
            .unwrap_or(sys_tenant_membership::TenantMembershipStatus::Enabled);
        let db = self.db.clone();
        let tenant_id = tenant_id.to_string();
        let operator = operator.to_string();
        db.transaction::<_, (), ApiErrors>(move |txn| {
            let tenant_id = tenant_id.clone();
            let operator = operator.clone();
            let dto = dto.clone();
            Box::pin(async move {
                if should_be_default
                    && effective_status == sys_tenant_membership::TenantMembershipStatus::Enabled
                {
                    let others = sys_tenant_membership::Entity::find()
                        .filter(sys_tenant_membership::Column::UserId.eq(dto.user_id))
                        .filter(sys_tenant_membership::Column::IsDefault.eq(true))
                        .filter(
                            sys_tenant_membership::Column::Status
                                .eq(sys_tenant_membership::TenantMembershipStatus::Enabled),
                        )
                        .all(txn)
                        .await
                        .context("查询默认租户成员关系失败")
                        .map_err(ApiErrors::Internal)?;

                    for item in others {
                        let mut active: sys_tenant_membership::ActiveModel = item.into();
                        active.is_default = Set(false);
                        active.update_by = Set(operator.clone());
                        active
                            .update(txn)
                            .await
                            .context("取消其他默认租户失败")
                            .map_err(ApiErrors::Internal)?;
                    }
                }

                if let Some(membership) = sys_tenant_membership::Entity::find()
                    .filter(sys_tenant_membership::Column::TenantId.eq(tenant_id.clone()))
                    .filter(sys_tenant_membership::Column::UserId.eq(dto.user_id))
                    .one(txn)
                    .await
                    .context("查询租户成员关系失败")
                    .map_err(ApiErrors::Internal)?
                {
                    let mut active: sys_tenant_membership::ActiveModel = membership.into();
                    dto.clone().apply_to(&mut active, &operator);
                    active
                        .update(txn)
                        .await
                        .context("更新租户成员关系失败")
                        .map_err(ApiErrors::Internal)?;
                } else {
                    dto.clone()
                        .into_active_model(tenant_id.clone(), operator.clone())
                        .insert(txn)
                        .await
                        .context("创建租户成员关系失败")
                        .map_err(ApiErrors::Internal)?;
                }

                Ok(())
            })
        })
        .await?;

        Ok(())
    }

    pub async fn provision_tenant(
        &self,
        tenant_id: &str,
        dto: ProvisionTenantDto,
        operator: &str,
    ) -> ApiResult<TenantProvisionResultVo> {
        let tenant = self.find_tenant_model(tenant_id).await?;
        let isolation = dto
            .isolation_level
            .unwrap_or(tenant.default_isolation_level);
        let base_tables = sanitize_base_tables(&dto.base_tables);
        let record = TenantMetadataRecord {
            tenant_id: tenant_id.to_string(),
            isolation_level: tenant_to_runtime_isolation(isolation),
            status: Some("active".to_string()),
            schema_name: dto.schema_name.clone(),
            datasource_name: dto.datasource_name.clone(),
            db_uri: dto.db_uri.clone(),
            db_enable_logging: dto.db_enable_logging,
            db_min_conns: dto.db_min_conns.and_then(|value| u32::try_from(value).ok()),
            db_max_conns: dto.db_max_conns.map(|value| value as u32),
            db_connect_timeout_ms: dto
                .db_connect_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_idle_timeout_ms: dto
                .db_idle_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_acquire_timeout_ms: dto
                .db_acquire_timeout_ms
                .and_then(|value| u64::try_from(value).ok()),
            db_test_before_acquire: dto.db_test_before_acquire,
        };

        validate_provision_request(&record, &base_tables)?;
        let lifecycle = TenantLifecycleManager;
        let plan = lifecycle.plan_onboard(&record, &base_tables);
        self.execute_sql_batch(&plan.resource_sql).await?;

        let datasource = self
            .save_tenant_datasource(
                tenant_id,
                SaveTenantDatasourceDto {
                    isolation_level: tenant_to_datasource_isolation(isolation),
                    status: Some(sys_tenant_datasource::TenantDatasourceStatus::Active),
                    schema_name: dto.schema_name,
                    datasource_name: dto.datasource_name,
                    db_uri: dto.db_uri,
                    db_enable_logging: dto.db_enable_logging,
                    db_min_conns: dto.db_min_conns,
                    db_max_conns: dto.db_max_conns,
                    db_connect_timeout_ms: dto.db_connect_timeout_ms,
                    db_idle_timeout_ms: dto.db_idle_timeout_ms,
                    db_acquire_timeout_ms: dto.db_acquire_timeout_ms,
                    db_test_before_acquire: dto.db_test_before_acquire,
                    readonly_config: dto.readonly_config,
                    extra_config: dto.extra_config,
                    remark: dto.remark,
                },
                operator,
            )
            .await?;
        self.change_tenant_status(
            tenant_id,
            ChangeTenantStatusDto {
                status: sys_tenant::TenantStatus::Enabled,
                datasource_status: Some(sys_tenant_datasource::TenantDatasourceStatus::Active),
            },
            operator,
        )
        .await?;

        Ok(TenantProvisionResultVo {
            tenant_id: tenant_id.to_string(),
            isolation_level: isolation,
            resource_sql: plan.resource_sql,
            datasource,
        })
    }

    pub async fn refresh_runtime_metadata(&self) -> ApiResult<TenantRuntimeRefreshVo> {
        self.sharding
            .reload_tenant_metadata(&self.db)
            .await
            .context("刷新租户元数据失败")?;
        let route_states = self.sharding.refresh_route_states().await;
        Ok(TenantRuntimeRefreshVo {
            tenant_metadata_count: self.sharding.tenant_metadata_store().list().len(),
            datasource_count: self.sharding.datasource_names().len(),
            route_state_count: route_states.len(),
        })
    }

    pub async fn runtime_health(&self) -> ApiResult<Vec<TenantRuntimeDatasourceVo>> {
        let health = self.sharding.health_check().await;
        Ok(health.into_iter().map(map_runtime_health).collect())
    }

    pub async fn runtime_routes(&self) -> ApiResult<Vec<TenantRouteStateVo>> {
        let routes = self.sharding.refresh_route_states().await;
        Ok(routes.into_iter().map(map_route_state).collect())
    }

    async fn find_tenant_model(&self, tenant_id: &str) -> ApiResult<sys_tenant::Model> {
        sys_tenant::Entity::find()
            .filter(sys_tenant::Column::TenantId.eq(tenant_id))
            .one(&self.db)
            .await
            .context("查询租户失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("租户不存在: {tenant_id}")))
    }

    async fn find_tenant_datasource_model(
        &self,
        tenant_id: &str,
    ) -> ApiResult<Option<sys_tenant_datasource::Model>> {
        sys_tenant_datasource::Entity::find()
            .filter(sys_tenant_datasource::Column::TenantId.eq(tenant_id))
            .one(&self.db)
            .await
            .context("查询租户数据源失败")
            .map_err(ApiErrors::Internal)
    }

    async fn load_datasource_map(
        &self,
        tenant_ids: &[String],
    ) -> ApiResult<BTreeMap<String, sys_tenant_datasource::Model>> {
        if tenant_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let items = sys_tenant_datasource::Entity::find()
            .filter(sys_tenant_datasource::Column::TenantId.is_in(tenant_ids.to_vec()))
            .all(&self.db)
            .await
            .context("查询租户数据源列表失败")?;

        Ok(items
            .into_iter()
            .map(|item| (item.tenant_id.clone(), item))
            .collect())
    }

    async fn load_membership_count_map(
        &self,
        tenant_ids: &[String],
    ) -> ApiResult<BTreeMap<String, u64>> {
        if tenant_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let memberships = sys_tenant_membership::Entity::find()
            .filter(sys_tenant_membership::Column::TenantId.is_in(tenant_ids.to_vec()))
            .filter(
                sys_tenant_membership::Column::Status
                    .eq(sys_tenant_membership::TenantMembershipStatus::Enabled),
            )
            .all(&self.db)
            .await
            .context("查询租户成员数量失败")?;

        let mut counts = BTreeMap::new();
        for item in memberships {
            *counts.entry(item.tenant_id).or_insert(0) += 1;
        }
        Ok(counts)
    }

    async fn load_user_map(&self, user_ids: &[i64]) -> ApiResult<BTreeMap<i64, sys_user::Model>> {
        if user_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let items = sys_user::Entity::find()
            .filter(sys_user::Column::Id.is_in(user_ids.to_vec()))
            .all(&self.db)
            .await
            .context("查询用户信息失败")?;

        Ok(items.into_iter().map(|item| (item.id, item)).collect())
    }

    async fn execute_sql_batch(&self, statements: &[String]) -> ApiResult<()> {
        for statement in statements {
            self.db
                .execute_unprepared(statement)
                .await
                .with_context(|| format!("执行租户资源 SQL 失败: {statement}"))?;
        }
        Ok(())
    }
}

fn map_runtime_health(value: DataSourceHealth) -> TenantRuntimeDatasourceVo {
    TenantRuntimeDatasourceVo {
        datasource: value.datasource,
        reachable: value.reachable,
        error: value.error,
        latency_ms: value.latency_ms,
    }
}

fn map_route_state(value: DataSourceRouteState) -> TenantRouteStateVo {
    let effective_write_target = value.effective_write_target();
    TenantRouteStateVo {
        rule_name: value.rule_name,
        configured_primary: value.configured_primary,
        effective_write_target,
        healthy_replicas: value.healthy_replicas,
        unhealthy: value.unhealthy,
        failover_active: value.failover_active,
    }
}

fn datasource_status_for_tenant_status(
    status: sys_tenant::TenantStatus,
) -> sys_tenant_datasource::TenantDatasourceStatus {
    match status {
        sys_tenant::TenantStatus::Enabled => sys_tenant_datasource::TenantDatasourceStatus::Active,
        sys_tenant::TenantStatus::PendingProvision => {
            sys_tenant_datasource::TenantDatasourceStatus::Provisioning
        }
        sys_tenant::TenantStatus::Disabled | sys_tenant::TenantStatus::Archived => {
            sys_tenant_datasource::TenantDatasourceStatus::Inactive
        }
    }
}

fn tenant_to_runtime_isolation(
    value: sys_tenant::TenantIsolationLevel,
) -> RuntimeTenantIsolationLevel {
    match value {
        sys_tenant::TenantIsolationLevel::SharedRow => RuntimeTenantIsolationLevel::SharedRow,
        sys_tenant::TenantIsolationLevel::SeparateTable => {
            RuntimeTenantIsolationLevel::SeparateTable
        }
        sys_tenant::TenantIsolationLevel::SeparateSchema => {
            RuntimeTenantIsolationLevel::SeparateSchema
        }
        sys_tenant::TenantIsolationLevel::SeparateDatabase => {
            RuntimeTenantIsolationLevel::SeparateDatabase
        }
    }
}

fn tenant_to_datasource_isolation(
    value: sys_tenant::TenantIsolationLevel,
) -> sys_tenant_datasource::TenantIsolationLevel {
    match value {
        sys_tenant::TenantIsolationLevel::SharedRow => {
            sys_tenant_datasource::TenantIsolationLevel::SharedRow
        }
        sys_tenant::TenantIsolationLevel::SeparateTable => {
            sys_tenant_datasource::TenantIsolationLevel::SeparateTable
        }
        sys_tenant::TenantIsolationLevel::SeparateSchema => {
            sys_tenant_datasource::TenantIsolationLevel::SeparateSchema
        }
        sys_tenant::TenantIsolationLevel::SeparateDatabase => {
            sys_tenant_datasource::TenantIsolationLevel::SeparateDatabase
        }
    }
}

fn datasource_to_tenant_isolation(
    value: sys_tenant_datasource::TenantIsolationLevel,
) -> sys_tenant::TenantIsolationLevel {
    match value {
        sys_tenant_datasource::TenantIsolationLevel::SharedRow => {
            sys_tenant::TenantIsolationLevel::SharedRow
        }
        sys_tenant_datasource::TenantIsolationLevel::SeparateTable => {
            sys_tenant::TenantIsolationLevel::SeparateTable
        }
        sys_tenant_datasource::TenantIsolationLevel::SeparateSchema => {
            sys_tenant::TenantIsolationLevel::SeparateSchema
        }
        sys_tenant_datasource::TenantIsolationLevel::SeparateDatabase => {
            sys_tenant::TenantIsolationLevel::SeparateDatabase
        }
    }
}

fn sanitize_base_tables(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.to_string()))
        .map(|value| value.to_string())
        .collect()
}

fn validate_provision_request(
    record: &TenantMetadataRecord,
    base_tables: &[String],
) -> ApiResult<()> {
    match record.isolation_level {
        RuntimeTenantIsolationLevel::SeparateTable
        | RuntimeTenantIsolationLevel::SeparateSchema
            if base_tables.is_empty() =>
        {
            Err(ApiErrors::BadRequest(
                "独立表或独立 Schema 开通时必须提供 baseTables".to_string(),
            ))
        }
        RuntimeTenantIsolationLevel::SeparateDatabase if record.db_uri.is_none() => Err(
            ApiErrors::BadRequest("独立库开通时必须提供 dbUri".to_string()),
        ),
        _ => Ok(()),
    }
}

fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

#[cfg(test)]
mod tests {
    use super::{
        datasource_status_for_tenant_status, sanitize_base_tables, validate_provision_request,
    };
    use summer_sharding::{
        TenantIsolationLevel as RuntimeTenantIsolationLevel, TenantMetadataRecord,
    };
    use summer_system_model::entity::{sys_tenant, sys_tenant_datasource};

    #[test]
    fn datasource_status_mapping_follows_tenant_status() {
        assert_eq!(
            datasource_status_for_tenant_status(sys_tenant::TenantStatus::Enabled),
            sys_tenant_datasource::TenantDatasourceStatus::Active
        );
        assert_eq!(
            datasource_status_for_tenant_status(sys_tenant::TenantStatus::Disabled),
            sys_tenant_datasource::TenantDatasourceStatus::Inactive
        );
        assert_eq!(
            datasource_status_for_tenant_status(sys_tenant::TenantStatus::PendingProvision),
            sys_tenant_datasource::TenantDatasourceStatus::Provisioning
        );
        assert_eq!(
            datasource_status_for_tenant_status(sys_tenant::TenantStatus::Archived),
            sys_tenant_datasource::TenantDatasourceStatus::Inactive
        );
    }

    #[test]
    fn sanitize_base_tables_deduplicates_and_trims() {
        let values = sanitize_base_tables(&[
            " ai.log ".to_string(),
            "".to_string(),
            "ai.log".to_string(),
            "ai.request".to_string(),
        ]);
        assert_eq!(values, vec!["ai.log", "ai.request"]);
    }

    #[test]
    fn provision_validation_enforces_required_inputs() {
        let schema_error = validate_provision_request(
            &TenantMetadataRecord {
                tenant_id: "T-1".to_string(),
                isolation_level: RuntimeTenantIsolationLevel::SeparateSchema,
                status: Some("active".to_string()),
                schema_name: Some("tenant_t1".to_string()),
                datasource_name: None,
                db_uri: None,
                db_enable_logging: None,
                db_min_conns: None,
                db_max_conns: None,
                db_connect_timeout_ms: None,
                db_idle_timeout_ms: None,
                db_acquire_timeout_ms: None,
                db_test_before_acquire: None,
            },
            &[],
        )
        .expect_err("base tables required");
        assert!(schema_error.to_string().contains("baseTables"));

        let database_error = validate_provision_request(
            &TenantMetadataRecord {
                tenant_id: "T-2".to_string(),
                isolation_level: RuntimeTenantIsolationLevel::SeparateDatabase,
                status: Some("active".to_string()),
                schema_name: None,
                datasource_name: Some("tenant_t2".to_string()),
                db_uri: None,
                db_enable_logging: None,
                db_min_conns: None,
                db_max_conns: None,
                db_connect_timeout_ms: None,
                db_idle_timeout_ms: None,
                db_acquire_timeout_ms: None,
                db_test_before_acquire: None,
            },
            &[],
        )
        .expect_err("db uri required");
        assert!(database_error.to_string().contains("dbUri"));
    }
}
