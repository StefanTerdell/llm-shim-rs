use indexmap::IndexMap;
use serde::Deserialize;

use crate::apis::transcriptions::models::api::response::{
    non_streaming::{TranscriptionsResponseBody, TranscriptionsUsage},
    streaming::{
        ChatCompletionTranscriptionsChunk, CompatTranscriptionsChunk, TextTranscriptionsChunk,
        TranscriptionsStreamEvent,
    },
};

#[derive(Default)]
pub(crate) struct EventNormalizer {
    text: String,
    usage: Option<TranscriptionsUsage>,
    done: bool,
}

impl EventNormalizer {
    pub fn normalize(
        &mut self,
        event: TranscriptionsStreamEvent,
    ) -> Option<TranscriptionsStreamEvent> {
        match event {
            TranscriptionsStreamEvent::Delta { ref delta, .. } => {
                self.text.push_str(delta);
                Some(event)
            }
            TranscriptionsStreamEvent::Done { .. } => {
                self.done = true;
                Some(event)
            }
            TranscriptionsStreamEvent::Error { .. } => Some(event),
            TranscriptionsStreamEvent::Other(value) => {
                match CompatTranscriptionsChunk::deserialize(&value) {
                    Ok(CompatTranscriptionsChunk::ChatCompletion(chunk)) => {
                        self.normalize_chat_completion(chunk)
                    }
                    Ok(CompatTranscriptionsChunk::Text(chunk)) if chunk.event_type.is_none() => {
                        self.normalize_text(chunk)
                    }
                    _ => Some(TranscriptionsStreamEvent::Other(value)),
                }
            }
        }
    }

    pub fn finish(self) -> Option<TranscriptionsStreamEvent> {
        (!self.done).then(|| TranscriptionsStreamEvent::Done {
            text: self.text,
            usage: self.usage,
            additional_properties: IndexMap::new(),
        })
    }

    pub fn from_body(body: TranscriptionsResponseBody) -> Vec<TranscriptionsStreamEvent> {
        let (text, usage, additional_properties) = match body {
            TranscriptionsResponseBody::Json(body) => {
                let usage = body.usage_or_duration();
                (body.text, usage, body.additional_properties)
            }
            TranscriptionsResponseBody::Text(text) => (text, None, IndexMap::new()),
        };

        let delta = (!text.is_empty()).then(|| delta(text.clone()));

        delta
            .into_iter()
            .chain([TranscriptionsStreamEvent::Done {
                text,
                usage,
                additional_properties,
            }])
            .collect()
    }

    fn normalize_chat_completion(
        &mut self,
        chunk: ChatCompletionTranscriptionsChunk,
    ) -> Option<TranscriptionsStreamEvent> {
        if let Some(usage) = chunk.usage {
            self.usage = Some(usage.into());
        }

        let content = chunk
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.delta.content)?;

        self.push(content)
    }

    fn normalize_text(
        &mut self,
        chunk: TextTranscriptionsChunk,
    ) -> Option<TranscriptionsStreamEvent> {
        if let Some(usage) = chunk.usage_or_duration() {
            self.usage = Some(usage);
        }

        self.push(chunk.text)
    }

    fn push(&mut self, text: String) -> Option<TranscriptionsStreamEvent> {
        if text.is_empty() {
            return None;
        }

        self.text.push_str(&text);
        Some(delta(text))
    }
}

fn delta(text: String) -> TranscriptionsStreamEvent {
    TranscriptionsStreamEvent::Delta {
        delta: text,
        additional_properties: IndexMap::new(),
    }
}
