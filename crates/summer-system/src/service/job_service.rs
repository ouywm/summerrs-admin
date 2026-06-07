use summer::plugin::service::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::pagination::{Page, Pagination};
use summer_system_model::dto::job::{
    CreateJobDto, JobKeyQueryDto, JobListQueryDto, JobTaskQueryDto, TriggerJobDto, UpdateJobDto,
};
use summer_system_model::vo::job::{RatchJobTaskVo, RatchJobVo, RatchNamespaceVo};

use crate::job::ratch_client::RatchJobClient;

#[derive(Clone, Service)]
pub struct JobService {
    #[inject(component)]
    ratch: RatchJobClient,
}

impl JobService {
    pub async fn list_jobs(
        &self,
        query: JobListQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RatchJobVo>> {
        self.ratch.list_jobs(&query, &pagination).await
    }

    pub async fn get_job(&self, id: u64) -> ApiResult<RatchJobVo> {
        validate_id(id)?;
        self.ratch.job_info(id).await
    }

    pub async fn create_job(&self, dto: CreateJobDto) -> ApiResult<()> {
        dto.validate_semantics().map_err(ApiErrors::BadRequest)?;
        self.ratch.create_job(&dto).await
    }

    pub async fn update_job(&self, id: u64, dto: UpdateJobDto) -> ApiResult<()> {
        validate_id(id)?;
        let dto = dto.with_id(id);
        dto.validate_semantics().map_err(ApiErrors::BadRequest)?;
        self.ratch.update_job(&dto).await
    }

    pub async fn remove_job(&self, id: u64) -> ApiResult<()> {
        validate_id(id)?;
        self.ratch.remove_job(id).await
    }

    pub async fn trigger_job(&self, id: u64, dto: TriggerJobDto) -> ApiResult<()> {
        validate_id(id)?;
        self.ratch.trigger_job(id, &dto).await
    }

    pub async fn enable_job(&self, id: u64) -> ApiResult<()> {
        validate_id(id)?;
        self.ratch.enable_job(id).await
    }

    pub async fn disable_job(&self, id: u64) -> ApiResult<()> {
        validate_id(id)?;
        self.ratch.disable_job(id).await
    }

    pub async fn task_list(
        &self,
        id: u64,
        query: JobTaskQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RatchJobTaskVo>> {
        validate_id(id)?;
        self.ratch
            .task_list(&query.with_job_id(id), &pagination)
            .await
    }

    pub async fn latest_history(
        &self,
        id: u64,
        query: JobTaskQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RatchJobTaskVo>> {
        validate_id(id)?;
        self.ratch
            .latest_history(&query.with_job_id(id), &pagination)
            .await
    }

    pub async fn query_id_by_key(&self, query: JobKeyQueryDto) -> ApiResult<u64> {
        self.ratch.query_id_by_key(&query).await
    }

    pub async fn query_job_by_key(&self, query: JobKeyQueryDto) -> ApiResult<RatchJobVo> {
        self.ratch.query_job_by_key(&query).await
    }

    pub async fn list_namespaces(&self) -> ApiResult<Vec<RatchNamespaceVo>> {
        self.ratch.list_namespaces().await
    }

    pub async fn list_apps(&self) -> ApiResult<Vec<String>> {
        self.ratch.list_apps().await
    }
}

fn validate_id(id: u64) -> ApiResult<()> {
    if id == 0 {
        return Err(ApiErrors::BadRequest("任务ID必须大于0".to_string()));
    }
    Ok(())
}
