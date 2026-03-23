use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::ApiResult;
use summer_domain::dict::DictDomainService;
use summer_system_model::dto::sys_dict::{
    CreateDictDataDto, CreateDictTypeDto, DictDataQueryDto, DictTypeQueryDto, UpdateDictDataDto,
    UpdateDictTypeDto,
};
use summer_system_model::entity::{sys_dict_data, sys_dict_type};
use summer_system_model::vo::sys_dict::{DictDataSimpleVo, DictDataVo, DictTypeVo};

use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysDictService {
    #[inject(component)]
    db: DbConn,
}

impl SysDictService {
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

        Ok(page.map(DictTypeVo::from))
    }

    pub async fn create_dict_type(
        &self,
        dto: CreateDictTypeDto,
        operator: &str,
    ) -> ApiResult<DictTypeVo> {
        self.domain().create_dict_type(dto, operator).await
    }

    pub async fn update_dict_type(
        &self,
        id: i64,
        dto: UpdateDictTypeDto,
        operator: &str,
    ) -> ApiResult<DictTypeVo> {
        self.domain().update_dict_type(id, dto, operator).await
    }

    pub async fn delete_dict_type(&self, id: i64) -> ApiResult<i64> {
        self.domain().delete_dict_type(id).await
    }

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

        Ok(page.map(DictDataVo::from))
    }

    pub async fn get_dict_data_by_type(&self, dict_type: &str) -> ApiResult<Vec<DictDataSimpleVo>> {
        self.domain().get_dict_data_by_type(dict_type).await
    }

    pub async fn get_all_dict_data(
        &self,
    ) -> ApiResult<std::collections::HashMap<String, Vec<DictDataSimpleVo>>> {
        self.domain().get_all_dict_data().await
    }

    pub async fn create_dict_data(
        &self,
        dto: CreateDictDataDto,
        operator: &str,
    ) -> ApiResult<DictDataVo> {
        self.domain().create_dict_data(dto, operator).await
    }

    pub async fn update_dict_data(
        &self,
        id: i64,
        dto: UpdateDictDataDto,
        operator: &str,
    ) -> ApiResult<DictDataVo> {
        self.domain().update_dict_data(id, dto, operator).await
    }

    pub async fn delete_dict_data(&self, id: i64) -> ApiResult<i64> {
        self.domain().delete_dict_data(id).await
    }

    fn domain(&self) -> DictDomainService {
        DictDomainService::new(self.db.clone())
    }
}
