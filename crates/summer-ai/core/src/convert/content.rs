#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizedContentPart {
    Text(String),
    ImageData {
        mime_type: String,
        data: String,
    },
    ImageUrl {
        url: String,
        mime_type: Option<String>,
    },
}

pub fn extract_text_segments(content: &serde_json::Value) -> Option<String> {
    let text = normalize_openai_content_parts(content)
        .into_iter()
        .filter_map(|part| match part {
            NormalizedContentPart::Text(text) => Some(text),
            NormalizedContentPart::ImageData { .. } | NormalizedContentPart::ImageUrl { .. } => {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

pub fn joined_text_value(texts: Vec<String>) -> serde_json::Value {
    if texts.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(texts.join(""))
    }
}

pub fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let data = url.strip_prefix("data:")?;
    let (meta, payload) = data.split_once(',')?;
    let media_type = meta.strip_suffix(";base64")?;
    Some((media_type, payload))
}

pub fn normalize_openai_content_parts(content: &serde_json::Value) -> Vec<NormalizedContentPart> {
    match content {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => vec![NormalizedContentPart::Text(text.clone())],
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(normalize_openai_content_part)
            .collect(),
        serde_json::Value::Object(_) => {
            normalize_openai_content_part(content).into_iter().collect()
        }
        other => vec![NormalizedContentPart::Text(other.to_string())],
    }
}

fn normalize_openai_content_part(value: &serde_json::Value) -> Option<NormalizedContentPart> {
    match value {
        serde_json::Value::String(text) => Some(NormalizedContentPart::Text(text.clone())),
        serde_json::Value::Object(map) => match map.get("type").and_then(|value| value.as_str()) {
            Some("text") => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| NormalizedContentPart::Text(text.to_string())),
            Some("image_url") => {
                let image_url = map.get("image_url");
                let url = image_url
                    .and_then(|value| value.get("url"))
                    .and_then(|value| value.as_str())
                    .or_else(|| image_url.and_then(|value| value.as_str()))?;
                let mime_type = image_url
                    .and_then(|value| value.get("mime_type").or_else(|| value.get("mimeType")))
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);

                if let Some((mime_type, data)) = parse_data_url(url) {
                    Some(NormalizedContentPart::ImageData {
                        mime_type: mime_type.to_string(),
                        data: data.to_string(),
                    })
                } else {
                    Some(NormalizedContentPart::ImageUrl {
                        url: url.to_string(),
                        mime_type,
                    })
                }
            }
            _ => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| NormalizedContentPart::Text(text.to_string())),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_segments_joins_string_and_text_items() {
        assert_eq!(
            extract_text_segments(&serde_json::json!("hello world")),
            Some("hello world".to_string())
        );
        assert_eq!(
            extract_text_segments(&serde_json::json!([
                {"type": "text", "text": "line 1"},
                {"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}},
                {"type": "text", "text": "line 2"}
            ])),
            Some("line 1\nline 2".to_string())
        );
        assert_eq!(extract_text_segments(&serde_json::json!(null)), None);
    }

    #[test]
    fn joined_text_value_returns_null_for_empty_text() {
        assert_eq!(joined_text_value(Vec::new()), serde_json::Value::Null);
        assert_eq!(
            joined_text_value(vec!["hello".into(), " world".into()]),
            serde_json::json!("hello world")
        );
    }

    #[test]
    fn parse_data_url_splits_mime_type_and_payload() {
        assert_eq!(
            parse_data_url("data:image/png;base64,aGVsbG8="),
            Some(("image/png", "aGVsbG8="))
        );
        assert_eq!(parse_data_url("https://example.com/cat.png"), None);
    }

    #[test]
    fn normalize_openai_content_parts_understands_text_and_images() {
        assert_eq!(
            normalize_openai_content_parts(&serde_json::json!([
                {"type": "text", "text": "hello"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,aGVsbG8="}},
                {"type": "image_url", "image_url": {"url": "https://example.com/cat.png", "mime_type": "image/png"}}
            ])),
            vec![
                NormalizedContentPart::Text("hello".into()),
                NormalizedContentPart::ImageData {
                    mime_type: "image/png".into(),
                    data: "aGVsbG8=".into(),
                },
                NormalizedContentPart::ImageUrl {
                    url: "https://example.com/cat.png".into(),
                    mime_type: Some("image/png".into()),
                }
            ]
        );
    }
}
