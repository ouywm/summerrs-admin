use std::{convert::Infallible, time::Duration};

use async_stream::stream;
use futures::Stream;
use futures::StreamExt;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::providers::openai;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
use serde::Deserialize;
use summer_admin_macros::no_auth;
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::response::sse::{Event, KeepAlive, Sse};
use summer_web::handler::TypeRouter;
use summer_web::post;

#[derive(Debug, Deserialize)]
pub struct RigChatStreamRequest {
    pub prompt: String,
}

/// Rig 最小 SSE 对话接口。
///
/// SSE 事件说明：
/// - `delta`: 模型增量文本
/// - `done`: 流结束
/// - `error`: 流过程中出现错误
#[no_auth]
#[post("/system/rig/chat/stream")]
pub async fn rig_chat_stream(
    Json(dto): Json<RigChatStreamRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        let client = match openai::CompletionsClient::builder()
            .api_key("sk-TXFeLtWwzMTPoFJYY2JTXyTkFIAai9zFtMs1I1LzRhTwHx94")
            .base_url("https://sin.ioll.pp.ua/v1")
            .build() {
                Ok(client) => client,
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    return;
                }
            };

        let agent = client.agent("gpt-5.4").build();

        let mut response_stream = agent.stream_prompt(dto.prompt).await;

        while let Some(item) = response_stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
                    yield Ok(Event::default().event("delta").data(text.text));
                }
                Ok(MultiTurnStreamItem::FinalResponse(_)) => {
                    yield Ok(Event::default().event("done").data("[DONE]"));
                    break;
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    break;
                }
                _ => {}
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

pub fn routes(router: Router) -> Router {
    router.typed_route(rig_chat_stream)
}
