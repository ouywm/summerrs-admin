use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

pub async fn list_assistants(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            "/v1/assistants".into(),
            ResourceRequestSpec {
                endpoint_scope: "assistants",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
        )
        .await
}

/// POST /v1/assistants
#[post_api("/v1/assistants")]
pub async fn create_assistant(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            "/v1/assistants".into(),
            ResourceRequestSpec {
                endpoint_scope: "assistants",
                bind_resource_kind: Some("assistant"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/assistants/{assistant_id}
#[get_api("/v1/assistants/{assistant_id}")]
pub async fn get_assistant(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/assistants/{assistant_id}"),
            ResourceRequestSpec {
                endpoint_scope: "assistants",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("assistant", assistant_id)],
        )
        .await
}

/// POST /v1/assistants/{assistant_id}
#[post_api("/v1/assistants/{assistant_id}")]
pub async fn update_assistant(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/assistants/{assistant_id}"),
            ResourceRequestSpec {
                endpoint_scope: "assistants",
                bind_resource_kind: Some("assistant"),
                delete_resource_kind: None,
            },
            vec![("assistant", assistant_id)],
            None,
        )
        .await
}

/// DELETE /v1/assistants/{assistant_id}
#[delete_api("/v1/assistants/{assistant_id}")]
pub async fn delete_assistant(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/assistants/{assistant_id}"),
            ResourceRequestSpec {
                endpoint_scope: "assistants",
                bind_resource_kind: None,
                delete_resource_kind: Some("assistant"),
            },
            vec![("assistant", assistant_id.clone())],
            Some(("assistant", assistant_id)),
        )
        .await
}

/// POST /v1/threads
#[post_api("/v1/threads")]
pub async fn create_thread(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            "/v1/threads".into(),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("thread"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/threads/{thread_id}
#[get_api("/v1/threads/{thread_id}")]
pub async fn get_thread(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("thread", thread_id)],
        )
        .await
}

/// POST /v1/threads/{thread_id}
#[post_api("/v1/threads/{thread_id}")]
pub async fn update_thread(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("thread"),
                delete_resource_kind: None,
            },
            vec![("thread", thread_id)],
            None,
        )
        .await
}

/// DELETE /v1/threads/{thread_id}
#[delete_api("/v1/threads/{thread_id}")]
pub async fn delete_thread(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: Some("thread"),
            },
            vec![("thread", thread_id.clone())],
            Some(("thread", thread_id)),
        )
        .await
}

/// GET /v1/threads/{thread_id}/messages
#[get_api("/v1/threads/{thread_id}/messages")]
pub async fn list_thread_messages(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/messages"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("thread", thread_id)],
        )
        .await
}

/// POST /v1/threads/{thread_id}/messages
#[post_api("/v1/threads/{thread_id}/messages")]
pub async fn create_thread_message(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}/messages"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("message"),
                delete_resource_kind: None,
            },
            vec![("thread", thread_id)],
            None,
        )
        .await
}

/// GET /v1/threads/{thread_id}/messages/{message_id}
#[get_api("/v1/threads/{thread_id}/messages/{message_id}")]
pub async fn get_thread_message(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, message_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/messages/{message_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("message", message_id), ("thread", thread_id)],
        )
        .await
}

/// POST /v1/threads/{thread_id}/messages/{message_id}
#[post_api("/v1/threads/{thread_id}/messages/{message_id}")]
pub async fn update_thread_message(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, message_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}/messages/{message_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("message"),
                delete_resource_kind: None,
            },
            vec![("message", message_id), ("thread", thread_id)],
            None,
        )
        .await
}

/// GET /v1/threads/{thread_id}/runs
#[get_api("/v1/threads/{thread_id}/runs")]
pub async fn list_thread_runs(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/runs"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("thread", thread_id)],
        )
        .await
}

/// POST /v1/threads/{thread_id}/runs
#[post_api("/v1/threads/{thread_id}/runs")]
pub async fn create_thread_run(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_usage_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}/runs"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("run"),
                delete_resource_kind: None,
            },
            "threads/runs",
            "openai/threads_runs",
            vec![("thread", thread_id)],
            None,
        )
        .await
}

/// POST /v1/threads/runs
#[post_api("/v1/threads/runs")]
pub async fn create_thread_and_run(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_usage_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            "/v1/threads/runs".into(),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("run"),
                delete_resource_kind: None,
            },
            "threads/runs",
            "openai/threads_runs",
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}
#[get_api("/v1/threads/{thread_id}/runs/{run_id}")]
pub async fn get_thread_run(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/runs/{run_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("run", run_id), ("thread", thread_id)],
        )
        .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}
#[post_api("/v1/threads/{thread_id}/runs/{run_id}")]
pub async fn update_thread_run(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_usage_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}/runs/{run_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("run"),
                delete_resource_kind: None,
            },
            "threads/runs",
            "openai/threads_runs",
            vec![("run", run_id), ("thread", thread_id)],
            None,
        )
        .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs
#[post_api("/v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs")]
pub async fn submit_thread_run_tool_outputs(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_usage_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: Some("run"),
                delete_resource_kind: None,
            },
            "threads/runs/submit_tool_outputs",
            "openai/threads_runs_submit_tool_outputs",
            vec![("run", run_id), ("thread", thread_id)],
            None,
        )
        .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}/cancel
#[post_api("/v1/threads/{thread_id}/runs/{run_id}/cancel")]
pub async fn cancel_thread_run(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_bodyless_post(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/runs/{run_id}/cancel"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("run", run_id), ("thread", thread_id)],
        )
        .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}/steps
#[get_api("/v1/threads/{thread_id}/runs/{run_id}/steps")]
pub async fn list_thread_run_steps(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/runs/{run_id}/steps"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("run", run_id), ("thread", thread_id)],
        )
        .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}
#[get_api("/v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}")]
pub async fn get_thread_run_step(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id, step_id)): Path<(String, String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}"),
            ResourceRequestSpec {
                endpoint_scope: "threads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("run", run_id), ("thread", thread_id)],
        )
        .await
}
