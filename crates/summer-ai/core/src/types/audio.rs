use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AudioSpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl AudioSpeechRequest {
    pub fn estimate_input_tokens(&self) -> i32 {
        ((self.input.len() as f64) / 4.0).ceil() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_speech_request_estimates_tokens() {
        let req: AudioSpeechRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o-mini-tts",
            "input": "hello world",
            "voice": "alloy"
        }))
        .unwrap();

        assert_eq!(req.estimate_input_tokens(), 3);
    }

    #[test]
    fn audio_speech_request_preserves_extra_fields() {
        let req: AudioSpeechRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o-mini-tts",
            "input": "hello world",
            "voice": "alloy",
            "custom_field": "x"
        }))
        .unwrap();

        assert_eq!(req.extra.get("custom_field").unwrap(), "x");
    }
}
