use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::apis::{
    chat_completion::models::api::response::streaming::StreamingChatCompletionChunk,
    responses::models::api::{
        common::{ContentPart, OutputItem, ReasoningPart, ResponsesUsage},
        response::{non_streaming::ResponseBody, streaming::ResponsesStreamEvent},
    },
};

#[derive(Debug)]
enum OpenItem {
    Reasoning {
        index: u32,
        id: String,
        text: String,
    },
    Message {
        index: u32,
        id: String,
        text: String,
    },
    FunctionCall {
        index: u32,
        id: String,
        name: String,
        arguments: String,
        properties: IndexMap<String, Value>,
    },
}

impl OpenItem {
    fn index(&self) -> u32 {
        match self {
            Self::Reasoning { index, .. }
            | Self::Message { index, .. }
            | Self::FunctionCall { index, .. } => *index,
        }
    }
}

/// Turns a chat completion stream into the event sequence an OpenAI Responses client expects.
///
/// Stateful by necessity: Responses announces every output item and content part before its deltas
/// and closes both again afterwards, carries a running sequence number, and repeats the finished
/// text in its `done` events and in the final response body. A chat completion chunk carries none
/// of that. Feed every chunk to [`Self::chunk`] in order, then call [`Self::finish`].
#[derive(Debug, Default)]
pub struct ResponsesStreamConverter {
    sequence: u64,
    created: bool,
    next_index: u32,
    open: Option<OpenItem>,
    tool_items: IndexMap<u32, u32>,
    output: Vec<OutputItem>,
    usage: Option<ResponsesUsage>,
    response_id: Option<String>,
    model: Option<String>,
    failed: bool,
}

impl ResponsesStreamConverter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chunk(&mut self, chunk: StreamingChatCompletionChunk) -> Vec<ResponsesStreamEvent> {
        let StreamingChatCompletionChunk {
            error,
            choices,
            usage,
            additional_properties,
        } = chunk;

        let mut events = Vec::new();

        if self.response_id.is_none() {
            self.response_id = additional_properties
                .get("id")
                .and_then(Value::as_str)
                .map(|id| format!("resp_{id}"));
        }

        if self.model.is_none() {
            self.model = additional_properties
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string);
        }

        if let Some(error) = error {
            self.failed = true;
            events.push(ResponsesStreamEvent::Error {
                code: error.get("code").cloned(),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                param: error.get("param").cloned(),
                sequence_number: None,
                additional_properties: Default::default(),
            });
            self.number(&mut events);
            return events;
        }

        if !self.created {
            self.created = true;
            events.push(ResponsesStreamEvent::Created {
                response: self.response_body("in_progress"),
                sequence_number: None,
                additional_properties: Default::default(),
            });
            events.push(ResponsesStreamEvent::InProgress {
                response: self.response_body("in_progress"),
                sequence_number: None,
                additional_properties: Default::default(),
            });
        }

        if let Some(usage) = usage.as_ref() {
            self.usage = Some(ResponsesUsage::from(usage));
        }

        let Some(choice) = choices.into_iter().flatten().next() else {
            self.number(&mut events);
            return events;
        };

        let delta = choice.delta;

        if let Some(reasoning) = delta
            .common
            .reasoning_content()
            .or_else(|| delta.common.reasoning())
            .filter(|reasoning| !reasoning.is_empty())
        {
            self.open_reasoning(&mut events);

            if let Some(OpenItem::Reasoning { index, id, text }) = self.open.as_mut() {
                text.push_str(reasoning);

                events.push(ResponsesStreamEvent::ReasoningTextDelta {
                    item_id: Some(id.clone()),
                    output_index: *index,
                    content_index: 0,
                    delta: reasoning.to_string(),
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
        }

        if let Some(content) = delta.content().filter(|text| !text.is_empty()) {
            self.open_message(&mut events);

            if let Some(OpenItem::Message { index, id, text }) = self.open.as_mut() {
                text.push_str(content);

                events.push(ResponsesStreamEvent::OutputTextDelta {
                    item_id: Some(id.clone()),
                    output_index: *index,
                    content_index: 0,
                    delta: content.to_string(),
                    logprobs: None,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
        }

        for call in delta.common.tool_calls.into_iter().flatten() {
            let call_index = call.index.unwrap_or(0);

            if let Some(name) = call.function.name.filter(|name| !name.is_empty()) {
                let mut properties = call.additional_properties.clone();
                properties.shift_remove("type");

                if let Some(id) = properties.shift_remove("id") {
                    properties.insert("call_id".to_string(), id);
                }

                self.open_function_call(&mut events, call_index, name, properties);
            }

            if let Some(arguments) = call.function.arguments.filter(|a| !a.is_empty())
                && self.tool_items.get(&call_index).copied() == self.open.as_ref().map(OpenItem::index)
                && let Some(OpenItem::FunctionCall {
                    index,
                    arguments: accumulated,
                    ..
                }) = self.open.as_mut()
            {
                accumulated.push_str(&arguments);

                events.push(ResponsesStreamEvent::FunctionCallArgumentsDelta {
                    output_index: *index,
                    delta: arguments,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
        }

        self.number(&mut events);
        events
    }

    pub fn finish(mut self) -> Vec<ResponsesStreamEvent> {
        let mut events = Vec::new();

        if !self.created {
            self.created = true;
            events.push(ResponsesStreamEvent::Created {
                response: self.response_body("in_progress"),
                sequence_number: None,
                additional_properties: Default::default(),
            });
            events.push(ResponsesStreamEvent::InProgress {
                response: self.response_body("in_progress"),
                sequence_number: None,
                additional_properties: Default::default(),
            });
        }

        self.close_open_item(&mut events);

        let status = if self.failed { "failed" } else { "completed" };
        let response = self.response_body(status);

        events.push(if self.failed {
            ResponsesStreamEvent::Failed {
                response,
                sequence_number: None,
                additional_properties: Default::default(),
            }
        } else {
            ResponsesStreamEvent::Completed {
                response,
                sequence_number: None,
                additional_properties: Default::default(),
            }
        });

        self.number(&mut events);
        events
    }

    fn number(&mut self, events: &mut [ResponsesStreamEvent]) {
        for event in events.iter_mut() {
            if event.sequence_number().is_none() {
                event.set_sequence_number(self.sequence);
                self.sequence += 1;
            }
        }
    }

    fn response_body(&self, status: &str) -> ResponseBody {
        let mut additional_properties = IndexMap::from([
            ("object".to_string(), json!("response")),
            ("status".to_string(), json!(status)),
        ]);

        if let Some(id) = self.response_id.as_ref() {
            additional_properties.insert("id".to_string(), json!(id));
        }
        if let Some(model) = self.model.as_ref() {
            additional_properties.insert("model".to_string(), json!(model));
        }

        ResponseBody {
            output: self.output.clone(),
            usage: Some(self.usage.clone()),
            error: Some(None),
            additional_properties,
        }
    }

    fn close_open_item(&mut self, events: &mut Vec<ResponsesStreamEvent>) {
        let Some(open) = self.open.take() else {
            return;
        };

        match open {
            OpenItem::Reasoning { index, id, text } => {
                events.push(ResponsesStreamEvent::ReasoningTextDone {
                    item_id: Some(id.clone()),
                    output_index: index,
                    content_index: 0,
                    text: text.clone(),
                    sequence_number: None,
                    additional_properties: Default::default(),
                });

                let item = OutputItem::Reasoning {
                    id: Some(id),
                    summary: vec![],
                    content: Some(vec![ReasoningPart::reasoning_text(text)]),
                    additional_properties: Default::default(),
                };

                self.output.push(item.clone());
                events.push(ResponsesStreamEvent::OutputItemDone {
                    output_index: index,
                    item,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
            OpenItem::Message { index, id, text } => {
                events.push(ResponsesStreamEvent::OutputTextDone {
                    item_id: Some(id.clone()),
                    output_index: index,
                    content_index: 0,
                    text: text.clone(),
                    logprobs: None,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
                events.push(ResponsesStreamEvent::ContentPartDone {
                    item_id: Some(id.clone()),
                    output_index: index,
                    content_index: 0,
                    part: ContentPart::output_text(text.clone()),
                    sequence_number: None,
                    additional_properties: Default::default(),
                });

                let item = OutputItem::Message {
                    id: Some(id),
                    role: Some("assistant".to_string()),
                    status: Some("completed".to_string()),
                    content: vec![ContentPart::output_text(text)],
                    additional_properties: Default::default(),
                };

                self.output.push(item.clone());
                events.push(ResponsesStreamEvent::OutputItemDone {
                    output_index: index,
                    item,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
            OpenItem::FunctionCall {
                index,
                id,
                name,
                arguments,
                properties,
            } => {
                events.push(ResponsesStreamEvent::FunctionCallArgumentsDone {
                    output_index: index,
                    arguments: arguments.clone(),
                    sequence_number: None,
                    additional_properties: Default::default(),
                });

                let item = OutputItem::FunctionCall {
                    id: Some(id),
                    name,
                    arguments,
                    additional_properties: properties,
                };

                self.output.push(item.clone());
                events.push(ResponsesStreamEvent::OutputItemDone {
                    output_index: index,
                    item,
                    sequence_number: None,
                    additional_properties: Default::default(),
                });
            }
        }
    }

    fn open_reasoning(&mut self, events: &mut Vec<ResponsesStreamEvent>) {
        if matches!(self.open, Some(OpenItem::Reasoning { .. })) {
            return;
        }

        self.close_open_item(events);

        let index = self.take_index();
        let id = format!("rs_{index}");

        events.push(ResponsesStreamEvent::OutputItemAdded {
            output_index: index,
            item: OutputItem::reasoning(Some(id.clone())),
            sequence_number: None,
            additional_properties: Default::default(),
        });

        self.open = Some(OpenItem::Reasoning {
            index,
            id,
            text: String::new(),
        });
    }

    fn open_message(&mut self, events: &mut Vec<ResponsesStreamEvent>) {
        if matches!(self.open, Some(OpenItem::Message { .. })) {
            return;
        }

        self.close_open_item(events);

        let index = self.take_index();
        let id = format!("msg_{index}");

        events.push(ResponsesStreamEvent::OutputItemAdded {
            output_index: index,
            item: OutputItem::message(Some(id.clone())),
            sequence_number: None,
            additional_properties: Default::default(),
        });
        events.push(ResponsesStreamEvent::ContentPartAdded {
            item_id: Some(id.clone()),
            output_index: index,
            content_index: 0,
            part: ContentPart::output_text(""),
            sequence_number: None,
            additional_properties: Default::default(),
        });

        self.open = Some(OpenItem::Message {
            index,
            id,
            text: String::new(),
        });
    }

    fn open_function_call(
        &mut self,
        events: &mut Vec<ResponsesStreamEvent>,
        call_index: u32,
        name: String,
        properties: IndexMap<String, Value>,
    ) {
        self.close_open_item(events);

        let index = self.take_index();
        let id = format!("fc_{index}");

        self.tool_items.insert(call_index, index);

        events.push(ResponsesStreamEvent::OutputItemAdded {
            output_index: index,
            item: OutputItem::FunctionCall {
                id: Some(id.clone()),
                name: name.clone(),
                arguments: String::new(),
                additional_properties: properties.clone(),
            },
            sequence_number: None,
            additional_properties: Default::default(),
        });

        self.open = Some(OpenItem::FunctionCall {
            index,
            id,
            name,
            arguments: String::new(),
            properties,
        });
    }

    fn take_index(&mut self) -> u32 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: Vec<Value>) -> Vec<Value> {
        let mut converter = ResponsesStreamConverter::new();
        let mut events = Vec::new();

        for chunk in chunks {
            events.extend(converter.chunk(serde_json::from_value(chunk).unwrap()));
        }

        events
            .into_iter()
            .chain(converter.finish())
            .map(|event| serde_json::to_value(event).unwrap())
            .collect()
    }

    fn text_chunk(text: &str) -> Value {
        json!({"choices": [{"index": 0, "delta": {"content": text}}]})
    }

    fn types(events: &[Value]) -> Vec<&str> {
        events
            .iter()
            .map(|event| event["type"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn a_text_stream_gets_the_full_item_and_part_lifecycle() {
        let events = run(vec![text_chunk("Hel"), text_chunk("lo")]);

        assert_eq!(
            types(&events),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        assert_eq!(events[4]["delta"], json!("Hel"));
        assert_eq!(
            events[6]["text"],
            json!("Hello"),
            "the done event repeats the whole text, not the last fragment"
        );
    }

    #[test]
    fn every_event_is_numbered_in_order() {
        let events = run(vec![text_chunk("hi")]);

        let numbers: Vec<u64> = events
            .iter()
            .map(|event| event["sequence_number"].as_u64().unwrap())
            .collect();

        assert_eq!(numbers, (0..numbers.len() as u64).collect::<Vec<_>>());
    }

    #[test]
    fn reasoning_and_text_become_separate_output_items() {
        let events = run(vec![
            json!({"choices": [{"index": 0, "delta": {"reasoning_content": "hmm"}}]}),
            text_chunk("answer"),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.reasoning_text.delta",
                "response.reasoning_text.done",
                "response.output_item.done",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        assert_eq!(events[2]["item"]["type"], json!("reasoning"));
        assert_eq!(events[2]["output_index"], json!(0));
        assert_eq!(events[6]["item"]["type"], json!("message"));
        assert_eq!(events[6]["output_index"], json!(1));
    }

    #[test]
    fn a_streamed_tool_call_streams_its_arguments() {
        let events = run(vec![
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": ""}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "{\"q\":"}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "\"x\"}"}}]}}]}),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        assert_eq!(events[2]["item"]["type"], json!("function_call"));
        assert_eq!(events[2]["item"]["name"], json!("lookup"));
        assert_eq!(
            events[2]["item"]["call_id"],
            json!("call_1"),
            "responses names it call_id, and the client needs it to answer the call"
        );
        assert_eq!(events[5]["arguments"], json!("{\"q\":\"x\"}"));
    }

    #[test]
    fn the_final_response_carries_the_output_and_usage() {
        let events = run(vec![
            text_chunk("hi"),
            json!({"choices": [], "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15}}),
        ]);

        let completed = events.last().unwrap();
        assert_eq!(completed["type"], json!("response.completed"));
        assert_eq!(completed["response"]["status"], json!("completed"));
        assert_eq!(
            completed["response"]["usage"],
            json!({"input_tokens": 11, "output_tokens": 4, "total_tokens": 15})
        );
        assert_eq!(
            completed["response"]["output"][0]["content"][0]["text"],
            json!("hi"),
            "the terminal event repeats the whole output, which a client may read instead of the deltas"
        );
    }

    #[test]
    fn the_response_carries_an_id_and_model_derived_from_the_chunks() {
        let events = run(vec![
            json!({"id": "chatcmpl-1", "model": "m", "choices": [{"index": 0, "delta": {"content": "hi"}}]}),
        ]);

        assert_eq!(events[0]["response"]["id"], json!("resp_chatcmpl-1"));
        assert_eq!(events[0]["response"]["model"], json!("m"));
        assert_eq!(events[0]["response"]["status"], json!("in_progress"));
    }

    #[test]
    fn an_error_chunk_fails_the_response() {
        let events = run(vec![json!({"error": {"code": "server_error", "message": "boom"}})]);

        assert_eq!(events[0]["type"], json!("error"));
        assert_eq!(events[0]["message"], json!("boom"));
        assert_eq!(events.last().unwrap()["type"], json!("response.failed"));
        assert_eq!(
            events.last().unwrap()["response"]["status"],
            json!("failed")
        );
    }

    #[test]
    fn a_stream_that_sent_nothing_still_closes_properly() {
        let events = run(vec![]);

        assert_eq!(
            types(&events),
            vec!["response.created", "response.in_progress", "response.completed"]
        );
    }
}
