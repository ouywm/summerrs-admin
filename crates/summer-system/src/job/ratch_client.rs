use std::collections::HashMap;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::pagination::{Page, Pagination};
use summer_system_model::dto::job::{
    CreateJobDto, JobKeyQueryDto, JobListQueryDto, JobTaskQueryDto, RemoveJobRequest,
    TriggerJobDto, TriggerJobRequest, UpdateJobDto,
};
use summer_system_model::vo::job::{RatchJobTaskVo, RatchJobVo, RatchNamespaceVo};
use summer_xxl_job::XxlJobConfig;

const DEFAULT_TIMEOUT_MS: u64 = 3000;

#[derive(Clone)]
pub struct RatchJobClient {
    http: reqwest::Client,
    base_url: String,
    access_token: Option<String>,
    headers: HashMap<String, String>,
}

impl RatchJobClient {
    pub fn new(config: &XxlJobConfig) -> ApiResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
            .build()
            .map_err(|err| {
                ApiErrors::Internal(anyhow::anyhow!("构建RatchJob HTTP客户端失败: {err}"))
            })?;
        Ok(Self {
            http,
            base_url: infer_ratch_base_url(&config.admin_addresses)?,
            access_token: config.access_token.clone(),
            headers: config.headers.clone(),
        })
    }

    pub async fn create_job(&self, dto: &CreateJobDto) -> ApiResult<()> {
        self.post_empty("/ratch/v1/job/create", dto).await
    }

    pub async fn update_job(&self, dto: &UpdateJobDto) -> ApiResult<()> {
        self.post_empty("/ratch/v1/job/update", dto).await
    }

    pub async fn remove_job(&self, id: u64) -> ApiResult<()> {
        self.post_empty("/ratch/v1/job/remove", &RemoveJobRequest { id })
            .await
    }

    pub async fn trigger_job(&self, id: u64, dto: &TriggerJobDto) -> ApiResult<()> {
        self.post_empty(
            "/ratch/v1/job/trigger",
            &TriggerJobRequest {
                id,
                trigger_param: dto.trigger_param.clone(),
            },
        )
        .await
    }

    pub async fn job_info(&self, id: u64) -> ApiResult<RatchJobVo> {
        self.get_data("/ratch/v1/job/info", &[("id".to_string(), id.to_string())])
            .await
    }

    pub async fn list_jobs(
        &self,
        query: &JobListQueryDto,
        pagination: &Pagination,
    ) -> ApiResult<Page<RatchJobVo>> {
        let page = self
            .get_data(
                "/ratch/v1/job/list",
                &query_pairs_with_pagination(query, pagination)?,
            )
            .await?;
        Ok(into_page(page, pagination))
    }

    pub async fn task_list(
        &self,
        query: &JobTaskQueryDto,
        pagination: &Pagination,
    ) -> ApiResult<Page<RatchJobTaskVo>> {
        let page = self
            .get_data(
                "/ratch/v1/job/task/list",
                &query_pairs_with_pagination(query, pagination)?,
            )
            .await?;
        Ok(into_page(page, pagination))
    }

    pub async fn latest_history(
        &self,
        query: &JobTaskQueryDto,
        pagination: &Pagination,
    ) -> ApiResult<Page<RatchJobTaskVo>> {
        let page = self
            .get_data(
                "/ratch/v1/job/task/latest-history",
                &query_pairs_with_pagination(query, pagination)?,
            )
            .await?;
        Ok(into_page(page, pagination))
    }

    pub async fn query_id_by_key(&self, query: &JobKeyQueryDto) -> ApiResult<u64> {
        self.get_data("/ratch/v1/job/queryIdByKey", &query_pairs(query)?)
            .await
    }

    pub async fn query_job_by_key(&self, query: &JobKeyQueryDto) -> ApiResult<RatchJobVo> {
        self.get_data("/ratch/v1/job/queryJobByKey", &query_pairs(query)?)
            .await
    }

    pub async fn list_namespaces(&self) -> ApiResult<Vec<RatchNamespaceVo>> {
        self.get_data("/ratch/v1/namespace/list", &[]).await
    }

    pub async fn list_apps(&self) -> ApiResult<Vec<String>> {
        self.get_data("/ratch/v1/app/list", &[]).await
    }

    pub async fn enable_job(&self, id: u64) -> ApiResult<()> {
        self.update_job(&UpdateJobDto::enable_request(id, true))
            .await
    }

    pub async fn disable_job(&self, id: u64) -> ApiResult<()> {
        self.update_job(&UpdateJobDto::enable_request(id, false))
            .await
    }

    async fn get_data<T>(&self, path: &str, query: &[(String, String)]) -> ApiResult<T>
    where
        T: DeserializeOwned,
    {
        let url = build_url(&self.base_url, path, query)?;
        let response = self
            .apply_headers(self.http.get(&url))
            .send()
            .await
            .map_err(|err| self.transport_error("GET", &url, err))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|err| self.transport_error("GET", &url, err))?;
        if !status.is_success() {
            tracing::warn!(%url, %status, body = %String::from_utf8_lossy(&body), "RatchJob HTTP请求失败");
            return Err(ApiErrors::ServiceUnavailable(format!(
                "RatchJob请求失败: HTTP {status}"
            )));
        }
        parse_data_envelope(&body).map_err(|err| self.decode_error("GET", &url, &body, err))
    }

    async fn post_empty<TReq>(&self, path: &str, request: &TReq) -> ApiResult<()>
    where
        TReq: Serialize + ?Sized,
    {
        let body = self.post_raw(path, request).await?;
        parse_empty_content_envelope(&body).map_err(|err| {
            self.decode_error("POST", &format!("{}{}", self.base_url, path), &body, err)
        })
    }

    async fn post_raw<TReq>(&self, path: &str, request: &TReq) -> ApiResult<bytes::Bytes>
    where
        TReq: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .apply_headers(self.http.post(&url))
            .json(request)
            .send()
            .await
            .map_err(|err| self.transport_error("POST", &url, err))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|err| self.transport_error("POST", &url, err))?;
        if !status.is_success() {
            tracing::warn!(%url, %status, body = %String::from_utf8_lossy(&body), "RatchJob HTTP请求失败");
            return Err(ApiErrors::ServiceUnavailable(format!(
                "RatchJob请求失败: HTTP {status}"
            )));
        }
        Ok(body)
    }

    fn transport_error(&self, method: &str, url: &str, err: reqwest::Error) -> ApiErrors {
        tracing::warn!(%method, %url, error = %err, "RatchJob请求异常");
        ApiErrors::ServiceUnavailable(format!("RatchJob服务不可用: {err}"))
    }

    fn apply_headers(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.access_token
            && !token.is_empty()
        {
            request = request.header("XXL-JOB-ACCESS-TOKEN", token);
        }
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }
        request
    }

    fn decode_error(
        &self,
        method: &str,
        url: &str,
        body: &[u8],
        err: RatchJobClientError,
    ) -> ApiErrors {
        tracing::warn!(
            %method,
            %url,
            error = %err,
            body = %String::from_utf8_lossy(body),
            "RatchJob响应解析失败"
        );
        match err {
            RatchJobClientError::Remote(message) => ApiErrors::ServiceUnavailable(message),
            RatchJobClientError::Decode(message) => {
                ApiErrors::Internal(anyhow::anyhow!("RatchJob响应解析失败: {message}"))
            }
        }
    }
}

pub struct RatchJobClientPlugin;

#[async_trait]
impl Plugin for RatchJobClientPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<XxlJobConfig>()
            .expect("xxl-job配置加载失败");
        let client = RatchJobClient::new(&config).expect("RatchJobClient初始化失败");
        app.add_component(client);
    }

    fn name(&self) -> &str {
        "ratch-job-client"
    }
}

#[derive(Debug, Deserialize)]
#[cfg(test)]
struct ContentEnvelope<T> {
    content: Option<T>,
    code: i64,
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmptyContentEnvelope {
    code: i64,
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DataEnvelope<T> {
    data: Option<T>,
    success: bool,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RatchPageVo<T> {
    total_count: u64,
    list: Vec<T>,
}

#[derive(Debug, thiserror::Error)]
enum RatchJobClientError {
    #[error("{0}")]
    Remote(String),
    #[error("{0}")]
    Decode(String),
}

#[cfg(test)]
fn parse_content_envelope<T>(body: &[u8]) -> Result<T, RatchJobClientError>
where
    T: DeserializeOwned,
{
    let envelope: ContentEnvelope<T> =
        serde_json::from_slice(body).map_err(|err| RatchJobClientError::Decode(err.to_string()))?;
    if envelope.code != 200 {
        return Err(RatchJobClientError::Remote(envelope.msg.unwrap_or_else(
            || format!("RatchJob返回失败状态: {}", envelope.code),
        )));
    }
    envelope
        .content
        .ok_or_else(|| RatchJobClientError::Decode("RatchJob响应缺少content字段".to_string()))
}

fn parse_empty_content_envelope(body: &[u8]) -> Result<(), RatchJobClientError> {
    let envelope: EmptyContentEnvelope =
        serde_json::from_slice(body).map_err(|err| RatchJobClientError::Decode(err.to_string()))?;
    if envelope.code != 200 {
        return Err(RatchJobClientError::Remote(envelope.msg.unwrap_or_else(
            || format!("RatchJob返回失败状态: {}", envelope.code),
        )));
    }
    Ok(())
}

fn parse_data_envelope<T>(body: &[u8]) -> Result<T, RatchJobClientError>
where
    T: DeserializeOwned,
{
    let envelope: DataEnvelope<T> =
        serde_json::from_slice(body).map_err(|err| RatchJobClientError::Decode(err.to_string()))?;
    if !envelope.success {
        let code = envelope.code.unwrap_or_else(|| "UNKNOWN".to_string());
        let message = envelope
            .message
            .unwrap_or_else(|| "RatchJob返回失败状态".to_string());
        return Err(RatchJobClientError::Remote(format!(
            "RatchJob调用失败({code}): {message}"
        )));
    }
    envelope
        .data
        .ok_or_else(|| RatchJobClientError::Decode("RatchJob响应缺少data字段".to_string()))
}

fn query_pairs<T>(value: &T) -> ApiResult<Vec<(String, String)>>
where
    T: Serialize,
{
    let value = serde_json::to_value(value)
        .map_err(|err| ApiErrors::Internal(anyhow::anyhow!("构建RatchJob查询参数失败: {err}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("RatchJob查询参数必须是JSON对象")))?;
    let mut pairs = Vec::with_capacity(object.len());
    for (key, value) in object {
        if value.is_null() {
            continue;
        }
        let value = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(v) => v.to_string(),
            serde_json::Value::Number(v) => v.to_string(),
            other => other.to_string(),
        };
        pairs.push((key.clone(), value));
    }
    Ok(pairs)
}

fn query_pairs_with_pagination<T>(
    value: &T,
    pagination: &Pagination,
) -> ApiResult<Vec<(String, String)>>
where
    T: Serialize,
{
    let mut pairs = query_pairs(value)?;
    pairs.push(("pageNo".to_string(), (pagination.page + 1).to_string()));
    pairs.push(("pageSize".to_string(), pagination.size.to_string()));
    Ok(pairs)
}

fn into_page<T>(page: RatchPageVo<T>, pagination: &Pagination) -> Page<T> {
    Page::new(page.list, pagination, page.total_count)
}

fn build_url(base_url: &str, path: &str, query: &[(String, String)]) -> ApiResult<String> {
    let mut url = url::Url::parse(&format!("{base_url}{path}"))
        .map_err(|err| ApiErrors::Internal(anyhow::anyhow!("构建RatchJob请求URL失败: {err}")))?;
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

fn infer_ratch_base_url(admin_addresses: &str) -> ApiResult<String> {
    let first = admin_addresses
        .split(',')
        .map(str::trim)
        .find(|addr| !addr.is_empty())
        .ok_or_else(|| ApiErrors::BadRequest("xxl-job.admin_addresses不能为空".to_string()))?;

    let without_trailing = first.trim_end_matches('/');
    let base = without_trailing
        .strip_suffix("/xxl-job-admin")
        .unwrap_or(without_trailing)
        .trim_end_matches('/');

    if base.is_empty() {
        return Err(ApiErrors::BadRequest(
            "无法从xxl-job.admin_addresses推导RatchJob地址".to_string(),
        ));
    }

    Ok(base.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_system_model::dto::job::JobScheduleType;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct DemoContent {
        id: u64,
        app_name: String,
    }

    #[test]
    fn parses_content_envelope_success() {
        let body = br#"{"content":{"id":2,"appName":"demo"},"code":200,"msg":null}"#;
        let parsed: DemoContent = parse_content_envelope(body).unwrap();
        assert_eq!(
            parsed,
            DemoContent {
                id: 2,
                app_name: "demo".to_string()
            }
        );
    }

    #[test]
    fn parses_content_envelope_failure() {
        let body = br#"{"content":{},"code":500,"msg":"bad job"}"#;
        let err = parse_empty_content_envelope(body).unwrap_err();
        assert!(err.to_string().contains("bad job"));
    }

    #[test]
    fn parses_data_envelope_success() {
        let body = br#"{"data":7,"success":true,"code":null,"message":null}"#;
        let parsed: u64 = parse_data_envelope(body).unwrap();
        assert_eq!(parsed, 7);
    }

    #[test]
    fn parses_job_list_with_empty_schedule_type_as_none() {
        let body = br#"{"data":{"totalCount":1,"list":[{"id":3,"enable":true,"appName":"summerrs-admin-executor","key":"e542e864651e4d8e81c3d423a4f5f6d7","namespace":"xxl","description":"debug","scheduleType":"","cronValue":"","delaySecond":0,"intervalSecond":0,"runMode":"BEAN","handleName":"summer_system::test_panic","triggerParam":"","routerStrategy":"ROUND_ROBIN","pastDueStrategy":"DEFAULT","blockingStrategy":"SERIAL_EXECUTION","timeoutSecond":0,"tryTimes":0,"versionId":0,"lastModifiedMillis":1780678110479,"registerTime":1780678110479,"retryInterval":0}]},"success":true,"code":null,"message":null}"#;

        let page: RatchPageVo<RatchJobVo> = parse_data_envelope(body).unwrap();

        assert_eq!(page.total_count, 1);
        assert_eq!(page.list[0].schedule_type, JobScheduleType::None);
    }

    #[test]
    fn parses_data_envelope_failure() {
        let body = br#"{"data":null,"success":false,"code":"500","message":"not found"}"#;
        let err = parse_data_envelope::<u64>(body).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn query_pairs_skip_nulls_and_keep_camel_case() {
        let query = JobListQueryDto {
            namespace: Some("xxl".to_string()),
            app_name: Some("demo".to_string()),
            like_description: None,
            like_handle_name: None,
        };
        let pairs = query_pairs_with_pagination(
            &query,
            &summer_sea_orm::pagination::Pagination {
                page: 0,
                size: 10,
                one_indexed: true,
            },
        )
        .unwrap();
        assert!(pairs.contains(&("appName".to_string(), "demo".to_string())));
        assert!(pairs.contains(&("pageNo".to_string(), "1".to_string())));
        assert!(pairs.contains(&("pageSize".to_string(), "10".to_string())));
        assert!(!pairs.iter().any(|(key, _)| key == "likeDescription"));
    }

    #[test]
    fn converts_ratch_page_to_project_page() {
        let ratch_page = RatchPageVo {
            total_count: 21,
            list: vec![1, 2],
        };
        let page = into_page(
            ratch_page,
            &summer_sea_orm::pagination::Pagination {
                page: 1,
                size: 10,
                one_indexed: true,
            },
        );
        assert_eq!(page.content, vec![1, 2]);
        assert_eq!(page.page, 2);
        assert_eq!(page.size, 10);
        assert_eq!(page.total_elements, 21);
        assert_eq!(page.total_pages, 3);
    }

    #[test]
    fn infers_ratch_base_url_from_xxl_admin_addresses() {
        let base = infer_ratch_base_url("http://127.0.0.1:8725/xxl-job-admin").unwrap();
        assert_eq!(base, "http://127.0.0.1:8725");

        let base = infer_ratch_base_url("http://ratchjob:8725/xxl-job-admin,http://x").unwrap();
        assert_eq!(base, "http://ratchjob:8725");
    }

    #[test]
    fn builds_url_with_query_pairs() {
        let url = build_url(
            "http://127.0.0.1:8725",
            "/ratch/v1/job/info",
            &[("id".to_string(), "7".to_string())],
        )
        .unwrap();
        assert_eq!(url, "http://127.0.0.1:8725/ratch/v1/job/info?id=7");
    }

    #[test]
    fn trigger_request_serializes_id_and_optional_trigger_param() {
        let dto = TriggerJobRequest {
            id: 9,
            trigger_param: Some("manual".to_string()),
        };
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"id": 9, "triggerParam": "manual"})
        );

        let dto = TriggerJobRequest {
            id: 9,
            trigger_param: None,
        };
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value, serde_json::json!({"id": 9}));
    }
}
