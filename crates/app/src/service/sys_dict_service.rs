use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_dict::{
    CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto, UpdateDictDataDto,
    UpdateDictTypeDto,
};
use model::entity::{sys_dict_data, sys_dict_type};
use model::vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use summer::plugin::Service;
use summer_sa_token::StpUtil;

use crate::plugin::sea_orm::pagination::{Page, Pagination, PaginationExt};
use crate::plugin::sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct SysDictService {
    #[inject(component)]
    db: DbConn,
}

impl SysDictService {
    /// 从 JWT payload 获取操作人昵称
    async fn get_operator_name(&self, login_id: &str) -> ApiResult<String> {
        let token = StpUtil::get_token_by_login_id(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let extra = StpUtil::get_extra_data(&token)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let name = extra
            .and_then(|v| {
                v.get("nick_name")
                    .and_then(|n| n.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("无法获取操作人昵称")))?;
        Ok(name)
    }

    /// 字典类型列表（分页+筛选）
    pub async fn list_dict_types(
        &self,
        query: DictTypeQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<DictTypeVo>> {
        let page = sys_dict_type::Entity::find()
            .filter(query)
            .page(&self.db, &pagination)
            .await
            .context("查询字典类型列表失败")?;

        let page = page.map(DictTypeVo::from);
        Ok(page)
    }

    /// 创建字典类型
    pub async fn create_dict_type(&self, dto: CreateDictTypeDto, login_id: &str) -> ApiResult<()> {
        let operator = self.get_operator_name(login_id).await?;

        // 检查字典类型编码是否已存在
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

        let dict_type = dto.into_active_model(operator);
        dict_type
            .insert(&self.db)
            .await
            .context("创建字典类型失败")?;
        Ok(())
    }

    /// 更新字典类型
    pub async fn update_dict_type(
        &self,
        id: i64,
        dto: UpdateDictTypeDto,
        login_id: &str,
    ) -> ApiResult<()> {
        let operator = self.get_operator_name(login_id).await?;

        let dict_type = sys_dict_type::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典类型失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典类型不存在".to_string()))?;

        // 检查是否系统内置
        if dict_type.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典类型不允许修改".to_string(),
            ));
        }

        let mut active: sys_dict_type::ActiveModel = dict_type.into();
        dto.apply_to(&mut active, &operator);
        active.update(&self.db).await.context("更新字典类型失败")?;
        Ok(())
    }

    /// 删除字典类型
    pub async fn delete_dict_type(&self, id: i64) -> ApiResult<()> {
        let dict_type = sys_dict_type::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典类型失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典类型不存在".to_string()))?;

        // 检查是否系统内置
        if dict_type.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典类型不允许删除".to_string(),
            ));
        }

        // 检查是否有关联的字典数据
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

        Ok(())
    }

    // ============================================================
    // 字典数据管理
    // ============================================================

    /// 字典数据列表（分页+筛选）
    pub async fn list_dict_data(
        &self,
        query: DictDataQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<DictDataVo>> {
        let page = sys_dict_data::Entity::find()
            .filter(query)
            .order_by_asc(sys_dict_data::Column::DictSort)
            .page(&self.db, &pagination)
            .await
            .context("查询字典数据列表失败")?;

        let page = page.map(DictDataVo::from);
        Ok(page)
    }

    /// 根据字典类型获取字典数据（简化版，用于前端下拉框）
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

    /// 获取全量字典数据（用于前端缓存）
    pub async fn get_all_dict_data(
        &self,
    ) -> ApiResult<std::collections::HashMap<String, Vec<DictDataSimpleVo>>> {
        let data = sys_dict_data::Entity::find()
            .filter(sys_dict_data::Column::Status.eq(sys_dict_type::DictStatus::Enabled))
            .order_by_asc(sys_dict_data::Column::DictSort)
            .all(&self.db)
            .await
            .context("查询全量字典数据失败")?;

        let mut result: std::collections::HashMap<String, Vec<DictDataSimpleVo>> =
            std::collections::HashMap::new();

        for item in data {
            let dict_type = item.dict_type.clone();
            let vo = DictDataSimpleVo::from(item);
            result.entry(dict_type).or_insert_with(Vec::new).push(vo);
        }

        Ok(result)
    }

    /// 创建字典数据
    pub async fn create_dict_data(&self, dto: CreateDictDataDto, login_id: &str) -> ApiResult<()> {
        let operator = self.get_operator_name(login_id).await?;

        // 检查字典类型是否存在
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

        // 检查字典值是否已存在
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

        let dict_data = dto.into_active_model(operator);
        dict_data
            .insert(&self.db)
            .await
            .context("创建字典数据失败")?;
        Ok(())
    }

    /// 更新字典数据
    pub async fn update_dict_data(
        &self,
        id: i64,
        dto: UpdateDictDataDto,
        login_id: &str,
    ) -> ApiResult<()> {
        let operator = self.get_operator_name(login_id).await?;

        let dict_data = sys_dict_data::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典数据失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典数据不存在".to_string()))?;

        // 检查是否系统内置
        if dict_data.is_system {
            return Err(ApiErrors::BadRequest(
                "系统内置字典数据不允许修改".to_string(),
            ));
        }

        let mut active: sys_dict_data::ActiveModel = dict_data.into();
        dto.apply_to(&mut active, &operator);
        active.update(&self.db).await.context("更新字典数据失败")?;
        Ok(())
    }

    /// 删除字典数据
    pub async fn delete_dict_data(&self, id: i64) -> ApiResult<()> {
        let dict_data = sys_dict_data::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询字典数据失败")?
            .ok_or_else(|| ApiErrors::NotFound("字典数据不存在".to_string()))?;

        // 检查是否系统内置
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

        Ok(())
    }
}
