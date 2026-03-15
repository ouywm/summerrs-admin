use std::collections::{HashMap, HashSet};

use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::{
    dto::sys_dict::{
        CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto,
        UpdateDictDataDto, UpdateDictTypeDto,
    },
    entity::{sys_dict_data, sys_dict_type},
    vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo},
};
use schemars::JsonSchema;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::sync::{SyncAction, SyncChange, SyncPlan};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictBundleItemSpec {
    pub dict_label: String,
    pub dict_value: String,
    pub dict_sort: Option<i32>,
    pub css_class: Option<String>,
    pub list_class: Option<String>,
    pub is_default: Option<bool>,
    pub status: Option<sys_dict_type::DictStatus>,
    pub remark: Option<String>,
}

impl DictBundleItemSpec {
    fn desired_sort(&self) -> i32 {
        self.dict_sort.unwrap_or(0)
    }

    fn desired_css_class(&self) -> String {
        self.css_class.clone().unwrap_or_default()
    }

    fn desired_list_class(&self) -> String {
        self.list_class.clone().unwrap_or_default()
    }

    fn desired_is_default(&self) -> bool {
        self.is_default.unwrap_or(false)
    }

    fn desired_status(&self) -> sys_dict_type::DictStatus {
        self.status.unwrap_or(sys_dict_type::DictStatus::Enabled)
    }

    fn desired_remark(&self) -> String {
        self.remark.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictBundleSpec {
    pub dict_name: String,
    pub dict_type: String,
    pub status: Option<sys_dict_type::DictStatus>,
    pub remark: Option<String>,
    #[serde(default)]
    pub items: Vec<DictBundleItemSpec>,
}

impl DictBundleSpec {
    fn desired_status(&self) -> sys_dict_type::DictStatus {
        self.status.unwrap_or(sys_dict_type::DictStatus::Enabled)
    }

    fn desired_remark(&self) -> String {
        self.remark.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DictBundleSyncResult {
    pub dict_type: String,
    pub plan: SyncPlan,
}

#[derive(Clone)]
pub struct DictDomainService {
    db: DatabaseConnection,
}

impl DictDomainService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list_dict_types(&self, query: DictTypeQueryDto) -> ApiResult<Vec<DictTypeVo>> {
        let items = sys_dict_type::Entity::find()
            .filter(query)
            .order_by_asc(sys_dict_type::Column::Id)
            .all(&self.db)
            .await
            .context("查询字典类型列表失败")?;
        Ok(items.into_iter().map(DictTypeVo::from).collect())
    }

    pub async fn plan_dict_bundle(&self, spec: &DictBundleSpec) -> ApiResult<DictBundleSyncResult> {
        validate_dict_bundle_spec(spec)?;

        let existing_type = sys_dict_type::Entity::find()
            .filter(sys_dict_type::Column::DictType.eq(&spec.dict_type))
            .one(&self.db)
            .await
            .context("查询字典类型失败")?;
        let existing_items = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::DictType.eq(&spec.dict_type))
            .order_by_asc(sys_dict_data::Column::DictSort)
            .all(&self.db)
            .await
            .context("查询字典数据失败")?;

        build_dict_bundle_plan(existing_type.as_ref(), &existing_items, spec)
    }

    pub async fn apply_dict_bundle(
        &self,
        spec: DictBundleSpec,
        operator: &str,
    ) -> ApiResult<DictBundleSyncResult> {
        validate_dict_bundle_spec(&spec)?;
        let operator = operator.to_string();

        self.db
            .transaction::<_, DictBundleSyncResult, ApiErrors>(|txn| {
                let spec = spec.clone();
                let operator = operator.clone();
                Box::pin(async move {
                    let existing_type = sys_dict_type::Entity::find()
                        .filter(sys_dict_type::Column::DictType.eq(&spec.dict_type))
                        .one(txn)
                        .await
                        .context("查询字典类型失败")
                        .map_err(ApiErrors::Internal)?;
                    let existing_items = sys_dict_data::Entity::find()
                        .filter(sys_dict_data::Column::DictType.eq(&spec.dict_type))
                        .order_by_asc(sys_dict_data::Column::DictSort)
                        .all(txn)
                        .await
                        .context("查询字典数据失败")
                        .map_err(ApiErrors::Internal)?;

                    let result =
                        build_dict_bundle_plan(existing_type.as_ref(), &existing_items, &spec)?;

                    apply_dict_type_spec(txn, existing_type.as_ref(), &spec, &operator).await?;

                    let existing_by_value = existing_items
                        .into_iter()
                        .map(|item| (item.dict_value.clone(), item))
                        .collect::<HashMap<_, _>>();

                    for item in &spec.items {
                        let existing = existing_by_value.get(&item.dict_value);
                        apply_dict_item_spec(txn, existing, &spec.dict_type, item, &operator)
                            .await?;
                    }

                    Ok(result)
                })
            })
            .await
            .map_err(map_transaction_error)
    }

    pub async fn create_dict_type(
        &self,
        dto: CreateDictTypeDto,
        operator: &str,
    ) -> ApiResult<DictTypeVo> {
        let existing = sys_dict_type::Entity::find()
            .filter(sys_dict_type::Column::DictType.eq(&dto.dict_type))
            .one(&self.db)
            .await
            .context("检查字典类型编码失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "字典类型编码已存在: {}",
                dto.dict_type
            )));
        }

        let dict_type = dto.into_active_model(operator.to_string());
        let model = dict_type
            .insert(&self.db)
            .await
            .context("创建字典类型失败")?;
        Ok(DictTypeVo::from(model))
    }

    pub async fn update_dict_type(
        &self,
        id: i64,
        dto: UpdateDictTypeDto,
        operator: &str,
    ) -> ApiResult<DictTypeVo> {
        let dict_type = sys_dict_type::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典类型失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典类型不存在".to_string()))?;

        if dict_type.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典类型不允许修改".to_string(),
            ));
        }

        let mut active: sys_dict_type::ActiveModel = dict_type.into();
        dto.apply_to(&mut active, operator);
        let model = active.update(&self.db).await.context("更新字典类型失败")?;
        Ok(DictTypeVo::from(model))
    }

    pub async fn delete_dict_type(&self, id: i64) -> ApiResult<i64> {
        let dict_type = sys_dict_type::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典类型失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典类型不存在".to_string()))?;

        if dict_type.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典类型不允许删除".to_string(),
            ));
        }

        let data_count = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::DictType.eq(&dict_type.dict_type))
            .count(&self.db)
            .await
            .context("查询字典数据关联失败")?;

        if data_count > 0 {
            return Err(ApiErrors::BadRequest(
                "该字典类型下存在字典数据，无法删除".to_string(),
            ));
        }

        let result = sys_dict_type::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除字典类型失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("字典类型不存在".to_string()));
        }

        Ok(id)
    }

    pub async fn list_dict_data(&self, query: DictDataQueryDto) -> ApiResult<Vec<DictDataVo>> {
        let items = sys_dict_data::Entity::find()
            .filter(query)
            .order_by_asc(sys_dict_data::Column::DictSort)
            .all(&self.db)
            .await
            .context("查询字典数据列表失败")?;
        Ok(items.into_iter().map(DictDataVo::from).collect())
    }

    pub async fn get_dict_data_by_type(&self, dict_type: &str) -> ApiResult<Vec<DictDataSimpleVo>> {
        let data = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::DictType.eq(dict_type))
            .filter(sys_dict_data::Column::Status.eq(sys_dict_type::DictStatus::Enabled))
            .order_by_asc(sys_dict_data::Column::DictSort)
            .all(&self.db)
            .await
            .context("查询字典数据失败")?;

        Ok(data.into_iter().map(DictDataSimpleVo::from).collect())
    }

    pub async fn get_all_dict_data(&self) -> ApiResult<HashMap<String, Vec<DictDataSimpleVo>>> {
        let data = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::Status.eq(sys_dict_type::DictStatus::Enabled))
            .order_by_asc(sys_dict_data::Column::DictSort)
            .all(&self.db)
            .await
            .context("查询全量字典数据失败")?;

        let mut result: HashMap<String, Vec<DictDataSimpleVo>> = HashMap::new();
        for item in data {
            let dict_type = item.dict_type.clone();
            result
                .entry(dict_type)
                .or_default()
                .push(DictDataSimpleVo::from(item));
        }
        Ok(result)
    }

    pub async fn create_dict_data(
        &self,
        dto: CreateDictDataDto,
        operator: &str,
    ) -> ApiResult<DictDataVo> {
        let dict_type_exists = sys_dict_type::Entity::find()
            .filter(sys_dict_type::Column::DictType.eq(&dto.dict_type))
            .one(&self.db)
            .await
            .context("查询字典类型失败")?;

        if dict_type_exists.is_none() {
            return Err(ApiErrors::BadRequest(format!(
                "字典类型不存在: {}",
                dto.dict_type
            )));
        }

        let existing = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::DictType.eq(&dto.dict_type))
            .filter(sys_dict_data::Column::DictValue.eq(&dto.dict_value))
            .one(&self.db)
            .await
            .context("检查字典值失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "字典值已存在: {}",
                dto.dict_value
            )));
        }

        let dict_data = dto.into_active_model(operator.to_string());
        let model = dict_data
            .insert(&self.db)
            .await
            .context("创建字典数据失败")?;
        Ok(DictDataVo::from(model))
    }

    pub async fn update_dict_data(
        &self,
        id: i64,
        dto: UpdateDictDataDto,
        operator: &str,
    ) -> ApiResult<DictDataVo> {
        let dict_data = sys_dict_data::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典数据失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典数据不存在".to_string()))?;

        if dict_data.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典数据不允许修改".to_string(),
            ));
        }

        let mut active: sys_dict_data::ActiveModel = dict_data.into();
        dto.apply_to(&mut active, operator);
        let model = active.update(&self.db).await.context("更新字典数据失败")?;
        Ok(DictDataVo::from(model))
    }

    pub async fn delete_dict_data(&self, id: i64) -> ApiResult<i64> {
        let dict_data = sys_dict_data::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典数据失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典数据不存在".to_string()))?;

        if dict_data.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典数据不允许删除".to_string(),
            ));
        }

        let result = sys_dict_data::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除字典数据失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("字典数据不存在".to_string()));
        }

        Ok(id)
    }
}

fn validate_dict_bundle_spec(spec: &DictBundleSpec) -> ApiResult<()> {
    if spec.dict_name.trim().is_empty() {
        return Err(ApiErrors::BadRequest("dict_name 不能为空".to_string()));
    }
    if spec.dict_type.trim().is_empty() {
        return Err(ApiErrors::BadRequest("dict_type 不能为空".to_string()));
    }

    let mut seen_values = HashSet::new();
    for item in &spec.items {
        if item.dict_label.trim().is_empty() {
            return Err(ApiErrors::BadRequest(
                "dict item 的 dict_label 不能为空".to_string(),
            ));
        }
        if item.dict_value.trim().is_empty() {
            return Err(ApiErrors::BadRequest(
                "dict item 的 dict_value 不能为空".to_string(),
            ));
        }
        if !seen_values.insert(item.dict_value.clone()) {
            return Err(ApiErrors::BadRequest(format!(
                "dict bundle 中存在重复的 dict_value: {}",
                item.dict_value
            )));
        }
    }

    Ok(())
}

fn build_dict_bundle_plan(
    existing_type: Option<&sys_dict_type::Model>,
    existing_items: &[sys_dict_data::Model],
    spec: &DictBundleSpec,
) -> ApiResult<DictBundleSyncResult> {
    validate_dict_bundle_spec(spec)?;

    let mut changes = Vec::with_capacity(spec.items.len() + 1);
    changes.push(build_dict_type_change(existing_type, spec));

    let existing_by_value = existing_items
        .iter()
        .map(|item| (item.dict_value.as_str(), item))
        .collect::<HashMap<_, _>>();

    for item in &spec.items {
        let existing = existing_by_value.get(item.dict_value.as_str()).copied();
        changes.push(build_dict_item_change(existing, &spec.dict_type, item));
    }

    Ok(DictBundleSyncResult {
        dict_type: spec.dict_type.clone(),
        plan: SyncPlan::new(changes),
    })
}

fn build_dict_type_change(
    existing: Option<&sys_dict_type::Model>,
    spec: &DictBundleSpec,
) -> SyncChange {
    let fields = match existing {
        Some(existing) => dict_type_changed_fields(existing, spec),
        None => vec!["dict_name", "status", "remark"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    };

    SyncChange {
        target: "dict_type".to_string(),
        key: spec.dict_type.clone(),
        action: if existing.is_none() {
            SyncAction::Create
        } else if fields.is_empty() {
            SyncAction::Noop
        } else {
            SyncAction::Update
        },
        fields,
    }
}

fn build_dict_item_change(
    existing: Option<&sys_dict_data::Model>,
    dict_type: &str,
    spec: &DictBundleItemSpec,
) -> SyncChange {
    let fields = match existing {
        Some(existing) => dict_item_changed_fields(existing, spec),
        None => vec![
            "dict_label",
            "dict_sort",
            "css_class",
            "list_class",
            "is_default",
            "status",
            "remark",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    };

    SyncChange {
        target: "dict_data".to_string(),
        key: format!("{dict_type}:{}", spec.dict_value),
        action: if existing.is_none() {
            SyncAction::Create
        } else if fields.is_empty() {
            SyncAction::Noop
        } else {
            SyncAction::Update
        },
        fields,
    }
}

fn dict_type_changed_fields(existing: &sys_dict_type::Model, spec: &DictBundleSpec) -> Vec<String> {
    let mut fields = Vec::new();
    if existing.dict_name != spec.dict_name {
        fields.push("dict_name".to_string());
    }
    if existing.status != spec.desired_status() {
        fields.push("status".to_string());
    }
    if existing.remark != spec.desired_remark() {
        fields.push("remark".to_string());
    }
    fields
}

fn dict_item_changed_fields(
    existing: &sys_dict_data::Model,
    spec: &DictBundleItemSpec,
) -> Vec<String> {
    let mut fields = Vec::new();
    if existing.dict_label != spec.dict_label {
        fields.push("dict_label".to_string());
    }
    if existing.dict_sort != spec.desired_sort() {
        fields.push("dict_sort".to_string());
    }
    if existing.css_class != spec.desired_css_class() {
        fields.push("css_class".to_string());
    }
    if existing.list_class != spec.desired_list_class() {
        fields.push("list_class".to_string());
    }
    if existing.is_default != spec.desired_is_default() {
        fields.push("is_default".to_string());
    }
    if existing.status != spec.desired_status() {
        fields.push("status".to_string());
    }
    if existing.remark != spec.desired_remark() {
        fields.push("remark".to_string());
    }
    fields
}

async fn apply_dict_type_spec<C: ConnectionTrait>(
    conn: &C,
    existing: Option<&sys_dict_type::Model>,
    spec: &DictBundleSpec,
    operator: &str,
) -> ApiResult<()> {
    if let Some(existing) = existing {
        let fields = dict_type_changed_fields(existing, spec);
        if fields.is_empty() {
            return Ok(());
        }
        if existing.is_system {
            return Err(ApiErrors::BadRequest(format!(
                "系统内置字典类型不允许通过 bundle 更新: {}",
                spec.dict_type
            )));
        }

        let mut active: sys_dict_type::ActiveModel = existing.clone().into();
        active.dict_name = Set(spec.dict_name.clone());
        active.status = Set(spec.desired_status());
        active.remark = Set(spec.desired_remark());
        active.update_by = Set(operator.to_string());
        active
            .update(conn)
            .await
            .context("更新字典类型失败")
            .map_err(ApiErrors::Internal)?;
        return Ok(());
    }

    sys_dict_type::ActiveModel {
        dict_name: Set(spec.dict_name.clone()),
        dict_type: Set(spec.dict_type.clone()),
        status: Set(spec.desired_status()),
        is_system: Set(false),
        remark: Set(spec.desired_remark()),
        create_by: Set(operator.to_string()),
        update_by: Set(operator.to_string()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("创建字典类型失败")
    .map_err(ApiErrors::Internal)?;

    Ok(())
}

async fn apply_dict_item_spec<C: ConnectionTrait>(
    conn: &C,
    existing: Option<&sys_dict_data::Model>,
    dict_type: &str,
    spec: &DictBundleItemSpec,
    operator: &str,
) -> ApiResult<()> {
    if let Some(existing) = existing {
        let fields = dict_item_changed_fields(existing, spec);
        if fields.is_empty() {
            return Ok(());
        }
        if existing.is_system {
            return Err(ApiErrors::BadRequest(format!(
                "系统内置字典数据不允许通过 bundle 更新: {}:{}",
                dict_type, spec.dict_value
            )));
        }

        let mut active: sys_dict_data::ActiveModel = existing.clone().into();
        active.dict_label = Set(spec.dict_label.clone());
        active.dict_sort = Set(spec.desired_sort());
        active.css_class = Set(spec.desired_css_class());
        active.list_class = Set(spec.desired_list_class());
        active.is_default = Set(spec.desired_is_default());
        active.status = Set(spec.desired_status());
        active.remark = Set(spec.desired_remark());
        active.update_by = Set(operator.to_string());
        active
            .update(conn)
            .await
            .context("更新字典数据失败")
            .map_err(ApiErrors::Internal)?;
        return Ok(());
    }

    sys_dict_data::ActiveModel {
        dict_type: Set(dict_type.to_string()),
        dict_label: Set(spec.dict_label.clone()),
        dict_value: Set(spec.dict_value.clone()),
        dict_sort: Set(spec.desired_sort()),
        css_class: Set(spec.desired_css_class()),
        list_class: Set(spec.desired_list_class()),
        is_default: Set(spec.desired_is_default()),
        status: Set(spec.desired_status()),
        is_system: Set(false),
        remark: Set(spec.desired_remark()),
        create_by: Set(operator.to_string()),
        update_by: Set(operator.to_string()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("创建字典数据失败")
    .map_err(ApiErrors::Internal)?;

    Ok(())
}

fn map_transaction_error(error: sea_orm::TransactionError<ApiErrors>) -> ApiErrors {
    match error {
        sea_orm::TransactionError::Connection(error) => {
            ApiErrors::Internal(anyhow::Error::new(error).context("数据库连接错误"))
        }
        sea_orm::TransactionError::Transaction(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict_type_model() -> sys_dict_type::Model {
        sys_dict_type::Model {
            id: 1,
            dict_name: "状态".to_string(),
            dict_type: "system_status".to_string(),
            status: sys_dict_type::DictStatus::Enabled,
            is_system: false,
            remark: String::new(),
            create_by: "seed".to_string(),
            create_time: chrono::NaiveDateTime::default(),
            update_by: "seed".to_string(),
            update_time: chrono::NaiveDateTime::default(),
        }
    }

    fn dict_item_model() -> sys_dict_data::Model {
        sys_dict_data::Model {
            id: 10,
            dict_type: "system_status".to_string(),
            dict_label: "启用".to_string(),
            dict_value: "enabled".to_string(),
            dict_sort: 0,
            css_class: String::new(),
            list_class: "success".to_string(),
            is_default: true,
            status: sys_dict_type::DictStatus::Enabled,
            is_system: false,
            remark: String::new(),
            create_by: "seed".to_string(),
            create_time: chrono::NaiveDateTime::default(),
            update_by: "seed".to_string(),
            update_time: chrono::NaiveDateTime::default(),
        }
    }

    #[test]
    fn build_dict_bundle_plan_marks_create_update_and_noop() {
        let spec = DictBundleSpec {
            dict_name: "系统状态".to_string(),
            dict_type: "system_status".to_string(),
            status: Some(sys_dict_type::DictStatus::Enabled),
            remark: None,
            items: vec![
                DictBundleItemSpec {
                    dict_label: "启用".to_string(),
                    dict_value: "enabled".to_string(),
                    dict_sort: Some(0),
                    css_class: None,
                    list_class: Some("success".to_string()),
                    is_default: Some(true),
                    status: Some(sys_dict_type::DictStatus::Enabled),
                    remark: None,
                },
                DictBundleItemSpec {
                    dict_label: "禁用".to_string(),
                    dict_value: "disabled".to_string(),
                    dict_sort: Some(1),
                    css_class: None,
                    list_class: Some("danger".to_string()),
                    is_default: Some(false),
                    status: Some(sys_dict_type::DictStatus::Enabled),
                    remark: None,
                },
            ],
        };

        let result = build_dict_bundle_plan(Some(&dict_type_model()), &[dict_item_model()], &spec)
            .expect("plan should build");

        assert_eq!(result.plan.summary.create_count, 1);
        assert_eq!(result.plan.summary.update_count, 1);
        assert_eq!(result.plan.summary.noop_count, 1);
        assert_eq!(result.plan.changes[0].action, SyncAction::Update);
        assert_eq!(result.plan.changes[1].action, SyncAction::Noop);
        assert_eq!(result.plan.changes[2].action, SyncAction::Create);
    }

    #[test]
    fn validate_dict_bundle_spec_rejects_duplicate_values() {
        let spec = DictBundleSpec {
            dict_name: "状态".to_string(),
            dict_type: "system_status".to_string(),
            status: None,
            remark: None,
            items: vec![
                DictBundleItemSpec {
                    dict_label: "启用".to_string(),
                    dict_value: "enabled".to_string(),
                    dict_sort: None,
                    css_class: None,
                    list_class: None,
                    is_default: None,
                    status: None,
                    remark: None,
                },
                DictBundleItemSpec {
                    dict_label: "启用2".to_string(),
                    dict_value: "enabled".to_string(),
                    dict_sort: None,
                    css_class: None,
                    list_class: None,
                    is_default: None,
                    status: None,
                    remark: None,
                },
            ],
        };

        let error = validate_dict_bundle_spec(&spec).expect_err("should reject duplicates");
        assert!(matches!(error, ApiErrors::BadRequest(_)));
    }
}
