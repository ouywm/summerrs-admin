use anyhow::Result;
use futures::StreamExt;
use futures::stream::BoxStream;

use crate::stream::sse_parser::SseParser;
use crate::types::chat::ChatCompletionChunk;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Clone)]
pub struct ChatStreamItem {
    chunk: Option<ChatCompletionChunk>,
    terminal: bool,
}

impl ChatStreamItem {
    pub fn chunk(chunk: ChatCompletionChunk) -> Self {
        Self {
            chunk: Some(chunk),
            terminal: false,
        }
    }

    pub fn terminal_chunk(chunk: ChatCompletionChunk) -> Self {
        Self {
            chunk: Some(chunk),
            terminal: true,
        }
    }

    pub fn terminal() -> Self {
        Self {
            chunk: None,
            terminal: true,
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    pub fn chunk_ref(&self) -> Option<&ChatCompletionChunk> {
        self.chunk.as_ref()
    }

    pub fn into_chunk(self) -> Option<ChatCompletionChunk> {
        self.chunk
    }
}

pub trait StreamEventMapper: Send + Sync {
    type State: Default + Send + 'static;

    fn map_event(&self, state: &mut Self::State, event: SseEvent) -> Vec<Result<ChatStreamItem>>;

    fn should_stop(&self, _state: &Self::State) -> bool {
        false
    }
}

pub fn sse_event_stream(response: reqwest::Response) -> BoxStream<'static, Result<SseEvent>> {
    let stream = async_stream::stream! {
        let mut byte_stream = response.bytes_stream();
        let mut parser = SseParser::new();

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(anyhow::anyhow!("Stream read error: {error}"));
                    break;
                }
            };

            let events = match parser.feed(&chunk) {
                Ok(events) => events,
                Err(error) => {
                    yield Err(error.context("failed to parse SSE event bytes"));
                    break;
                }
            };

            for event_text in events {
                if let Some(event) = parse_sse_event(&event_text) {
                    yield Ok(event);
                }
            }
        }
    };

    Box::pin(stream)
}

pub fn mapped_chunk_stream<M>(
    response: reqwest::Response,
    mapper: M,
) -> BoxStream<'static, Result<ChatStreamItem>>
where
    M: StreamEventMapper + 'static,
{
    let stream = async_stream::stream! {
        let mut state = M::State::default();
        let mut events = sse_event_stream(response);

        while let Some(event_result) = events.next().await {
            match event_result {
                Ok(event) => {
                    for chunk in mapper.map_event(&mut state, event) {
                        yield chunk;
                    }
                    if mapper.should_stop(&state) {
                        break;
                    }
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    };

    Box::pin(stream)
}

fn parse_sse_event(event_text: &str) -> Option<SseEvent> {
    let mut event_name = None;
    let mut data_lines = Vec::new();

    for line in event_text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim().to_string());
        }
    }

    if data_lines.is_empty() {
        None
    } else {
        Some(SseEvent {
            event: event_name,
            data: data_lines.join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::StreamExt;
    use futures::stream;

    use crate::types::chat::ChunkChoice;
    use crate::types::common::Delta;

    #[test]
    fn parse_event_collects_multiline_data() {
        let event = parse_sse_event("event: message\ndata: hello\ndata: world\n\n").unwrap();
        assert_eq!(event.event.as_deref(), Some("message"));
        assert_eq!(event.data, "hello\nworld");
    }

    struct StopAfterFirstMapper;

    impl StreamEventMapper for StopAfterFirstMapper {
        type State = usize;

        fn map_event(
            &self,
            state: &mut Self::State,
            event: SseEvent,
        ) -> Vec<Result<ChatStreamItem>> {
            *state += 1;
            vec![Ok(ChatStreamItem::chunk(ChatCompletionChunk {
                id: "stream-1".into(),
                object: "chat.completion.chunk".into(),
                created: 1,
                model: "demo".into(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: Some(event.data),
                        reasoning_content: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
                usage: None,
            }))]
        }

        fn should_stop(&self, state: &Self::State) -> bool {
            *state >= 1
        }
    }

    #[tokio::test]
    async fn mapped_chunk_stream_stops_when_mapper_requests_stop() {
        let chunks = vec![Ok::<_, std::io::Error>(Bytes::from_static(
            b"data: first\n\ndata: second\n\n",
        ))];
        let response = reqwest::Response::from(
            http::Response::builder()
                .status(200)
                .body(reqwest::Body::wrap_stream(stream::iter(chunks)))
                .unwrap(),
        );

        let items: Vec<_> = mapped_chunk_stream(response, StopAfterFirstMapper)
            .collect::<Vec<_>>()
            .await;

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_ref().unwrap().chunk_ref().unwrap().choices[0]
                .delta
                .content
                .as_deref(),
            Some("first")
        );
    }
}
