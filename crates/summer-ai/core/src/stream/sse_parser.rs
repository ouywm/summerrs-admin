use anyhow::{Context, Result};

#[derive(Debug, Default, Clone)]
pub struct SseParser {
    byte_buffer: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            byte_buffer: Vec::with_capacity(4096),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<String>> {
        self.byte_buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some((event_end, delimiter_len)) = find_sse_event_boundary(&self.byte_buffer) {
            let consumed: Vec<u8> = self
                .byte_buffer
                .drain(..event_end + delimiter_len)
                .collect();
            let event_bytes = &consumed[..event_end];
            if event_bytes.is_empty() {
                continue;
            }
            let event_text =
                String::from_utf8(event_bytes.to_vec()).context("invalid UTF-8 in SSE event")?;
            events.push(event_text);
        }

        Ok(events)
    }
}

fn find_sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some((index, 4));
        }
        if buffer[index..].starts_with(b"\n\n") || buffer[index..].starts_with(b"\r\r") {
            return Some((index, 2));
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_preserves_split_utf8_codepoint() {
        let mut parser = SseParser::new();
        let payload = "data: 你好\n\n".as_bytes();
        let split_at = payload
            .windows("你".len())
            .position(|window| window == "你".as_bytes())
            .expect("utf8 boundary")
            + 1;

        let first = parser.feed(&payload[..split_at]).unwrap();
        assert!(first.is_empty());

        let second = parser.feed(&payload[split_at..]).unwrap();
        assert_eq!(second, vec!["data: 你好".to_string()]);
    }

    #[test]
    fn parser_supports_crlf_boundaries() {
        let mut parser = SseParser::new();
        let events = parser.feed(b"data: hello\r\n\r\n").unwrap();
        assert_eq!(events, vec!["data: hello".to_string()]);
    }
}
