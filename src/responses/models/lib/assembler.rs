use indexmap::IndexMap;
use serde_json::Value;

use crate::responses::models::api::{
    common::{ContentPart, OutputItem, ReasoningPart},
    response::{non_streaming::ResponseBody, streaming::ResponsesStreamEvent},
};

fn slot<T>(vec: &mut Vec<T>, index: usize, make: impl Fn() -> T) -> &mut T {
    while vec.len() <= index {
        vec.push(make());
    }

    &mut vec[index]
}

#[derive(Default)]
pub struct ResponseAssembler {
    body: ResponseBody,
    items: IndexMap<u32, OutputItem>,
    error: Option<Value>,
}

impl ResponseAssembler {
    pub fn apply(&mut self, event: &ResponsesStreamEvent) {
        match event {
            ResponsesStreamEvent::Created { response, .. }
            | ResponsesStreamEvent::InProgress { response, .. } => {
                self.body = response.clone();
                self.body.output.clear();
            }
            ResponsesStreamEvent::Completed { response, .. }
            | ResponsesStreamEvent::Incomplete { response, .. }
            | ResponsesStreamEvent::Failed { response, .. } => {
                self.body = response.clone();
                let output = std::mem::take(&mut self.body.output);

                if !output.is_empty() {
                    self.items = output
                        .into_iter()
                        .enumerate()
                        .map(|(i, item)| (i as u32, item))
                        .collect();
                }

                if let Some(Some(error)) = &self.body.error
                    && !error.is_null()
                {
                    self.error = Some(error.clone());
                }
            }
            ResponsesStreamEvent::OutputItemAdded {
                output_index, item, ..
            }
            | ResponsesStreamEvent::OutputItemDone {
                output_index, item, ..
            } => {
                self.items.insert(*output_index, item.clone());
            }
            ResponsesStreamEvent::ContentPartAdded {
                output_index,
                content_index,
                part,
                ..
            }
            | ResponsesStreamEvent::ContentPartDone {
                output_index,
                content_index,
                part,
                ..
            } => match part {
                ContentPart::ReasoningText { text, .. } => {
                    *self.reasoning_content_mut(*output_index, *content_index) =
                        ReasoningPart::reasoning_text(text.clone());
                }
                part => {
                    *self.content_part_mut(*output_index, *content_index, || part.clone()) =
                        part.clone();
                }
            },
            ResponsesStreamEvent::OutputTextDelta {
                output_index,
                content_index,
                delta,
                ..
            } => {
                if let ContentPart::OutputText { text, .. } =
                    self.content_part_mut(*output_index, *content_index, || {
                        ContentPart::output_text("")
                    })
                {
                    text.push_str(delta);
                }
            }
            ResponsesStreamEvent::OutputTextDone {
                output_index,
                content_index,
                text: done,
                ..
            } => {
                if let ContentPart::OutputText { text, .. } =
                    self.content_part_mut(*output_index, *content_index, || {
                        ContentPart::output_text("")
                    })
                {
                    *text = done.clone();
                }
            }
            ResponsesStreamEvent::RefusalDelta {
                output_index,
                content_index,
                delta,
                ..
            } => {
                if let ContentPart::Refusal { refusal, .. } =
                    self.content_part_mut(*output_index, *content_index, || ContentPart::Refusal {
                        refusal: String::new(),
                        additional_properties: Default::default(),
                    })
                {
                    refusal.push_str(delta);
                }
            }
            ResponsesStreamEvent::RefusalDone {
                output_index,
                content_index,
                refusal: done,
                ..
            } => {
                if let ContentPart::Refusal { refusal, .. } =
                    self.content_part_mut(*output_index, *content_index, || ContentPart::Refusal {
                        refusal: String::new(),
                        additional_properties: Default::default(),
                    })
                {
                    *refusal = done.clone();
                }
            }
            ResponsesStreamEvent::ReasoningSummaryPartAdded {
                output_index,
                summary_index,
                part,
                ..
            }
            | ResponsesStreamEvent::ReasoningSummaryPartDone {
                output_index,
                summary_index,
                part,
                ..
            } => {
                *self.summary_part_mut(*output_index, *summary_index) = part.clone();
            }
            ResponsesStreamEvent::ReasoningSummaryTextDelta {
                output_index,
                summary_index,
                delta,
                ..
            } => {
                if let Some(text) = self
                    .summary_part_mut(*output_index, *summary_index)
                    .text_mut()
                {
                    text.push_str(delta);
                }
            }
            ResponsesStreamEvent::ReasoningSummaryTextDone {
                output_index,
                summary_index,
                text: done,
                ..
            } => {
                if let Some(text) = self
                    .summary_part_mut(*output_index, *summary_index)
                    .text_mut()
                {
                    *text = done.clone();
                }
            }
            ResponsesStreamEvent::ReasoningTextDelta {
                output_index,
                content_index,
                delta,
                ..
            } => {
                if let Some(text) = self
                    .reasoning_content_mut(*output_index, *content_index)
                    .text_mut()
                {
                    text.push_str(delta);
                }
            }
            ResponsesStreamEvent::ReasoningTextDone {
                output_index,
                content_index,
                text: done,
                ..
            } => {
                if let Some(text) = self
                    .reasoning_content_mut(*output_index, *content_index)
                    .text_mut()
                {
                    *text = done.clone();
                }
            }
            ResponsesStreamEvent::FunctionCallArgumentsDelta {
                output_index,
                delta,
                ..
            } => {
                if let OutputItem::FunctionCall { arguments, .. } =
                    self.item_or_insert(*output_index, || OutputItem::FunctionCall {
                        id: None,
                        name: String::new(),
                        arguments: String::new(),
                        additional_properties: Default::default(),
                    })
                {
                    arguments.push_str(delta);
                }
            }
            ResponsesStreamEvent::FunctionCallArgumentsDone {
                output_index,
                arguments: done,
                ..
            } => {
                if let OutputItem::FunctionCall { arguments, .. } =
                    self.item_or_insert(*output_index, || OutputItem::FunctionCall {
                        id: None,
                        name: String::new(),
                        arguments: String::new(),
                        additional_properties: Default::default(),
                    })
                {
                    *arguments = done.clone();
                }
            }
            ResponsesStreamEvent::Error {
                code,
                message,
                param,
                ..
            } => {
                self.error = Some(serde_json::json!({
                    "code": code,
                    "message": message,
                    "param": param,
                }));
            }
            ResponsesStreamEvent::Other(_) => {}
        }
    }

    pub fn item_mut(&mut self, output_index: u32) -> Option<&mut OutputItem> {
        self.items.get_mut(&output_index)
    }

    fn item_or_insert(
        &mut self,
        output_index: u32,
        make: impl FnOnce() -> OutputItem,
    ) -> &mut OutputItem {
        self.items.entry(output_index).or_insert_with(make)
    }

    fn content_part_mut(
        &mut self,
        output_index: u32,
        content_index: u32,
        make: impl Fn() -> ContentPart,
    ) -> &mut ContentPart {
        let item = self.item_or_insert(output_index, || OutputItem::message(None));

        if !matches!(item, OutputItem::Message { .. }) {
            *item = OutputItem::message(item.id().map(str::to_owned));
        }

        let OutputItem::Message { content, .. } = item else {
            unreachable!()
        };

        slot(content, content_index as usize, make)
    }

    fn summary_part_mut(&mut self, output_index: u32, summary_index: u32) -> &mut ReasoningPart {
        let item = self.item_or_insert(output_index, || OutputItem::reasoning(None));

        if !matches!(item, OutputItem::Reasoning { .. }) {
            *item = OutputItem::reasoning(item.id().map(str::to_owned));
        }

        let OutputItem::Reasoning { summary, .. } = item else {
            unreachable!()
        };

        slot(summary, summary_index as usize, || {
            ReasoningPart::summary_text("")
        })
    }

    fn reasoning_content_mut(
        &mut self,
        output_index: u32,
        content_index: u32,
    ) -> &mut ReasoningPart {
        let item = self.item_or_insert(output_index, || OutputItem::reasoning(None));

        if !matches!(item, OutputItem::Reasoning { .. }) {
            *item = OutputItem::reasoning(item.id().map(str::to_owned));
        }

        let OutputItem::Reasoning { content, .. } = item else {
            unreachable!()
        };

        slot(
            content.get_or_insert_default(),
            content_index as usize,
            || ReasoningPart::reasoning_text(""),
        )
    }

    pub fn item(&self, output_index: u32) -> Option<&OutputItem> {
        self.items.get(&output_index)
    }

    pub fn error(&self) -> Option<&Value> {
        self.error.as_ref()
    }

    pub fn output(&self) -> Vec<OutputItem> {
        let mut items: Vec<_> = self.items.iter().collect();
        items.sort_by_key(|(index, _)| **index);
        items.into_iter().map(|(_, item)| item.clone()).collect()
    }

    pub fn into_body(mut self) -> ResponseBody {
        self.body.output = self.output();
        self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::responses::models::api::common::{ContentPart, ReasoningPart};
    use serde_json::json;

    fn ev(v: Value) -> ResponsesStreamEvent {
        serde_json::from_value(v).unwrap()
    }

    fn assemble(events: Vec<Value>) -> ResponseAssembler {
        let mut a = ResponseAssembler::default();
        for v in events {
            a.apply(&ev(v));
        }
        a
    }

    #[test]
    fn full_openai_sequence_ends_with_the_completed_response() {
        let completed_output = json!([
            {"type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "think"}]},
            {"type": "message", "id": "msg_1", "status": "completed", "role": "assistant", "content": [{"type": "output_text", "text": "Hello", "annotations": []}]}
        ]);
        let a = assemble(vec![
            json!({"type": "response.created", "response": {"id": "resp_1", "status": "in_progress", "output": []}}),
            json!({"type": "response.output_item.added", "output_index": 0, "item": {"type": "reasoning", "id": "rs_1", "summary": []}}),
            json!({"type": "response.reasoning_summary_part.added", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "part": {"type": "summary_text", "text": ""}}),
            json!({"type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": "thi"}),
            json!({"type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": "nk"}),
            json!({"type": "response.reasoning_summary_text.done", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "text": "think"}),
            json!({"type": "response.reasoning_summary_part.done", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "part": {"type": "summary_text", "text": "think"}}),
            json!({"type": "response.output_item.done", "output_index": 0, "item": completed_output[0]}),
            json!({"type": "response.output_item.added", "output_index": 1, "item": {"type": "message", "id": "msg_1", "status": "in_progress", "role": "assistant", "content": []}}),
            json!({"type": "response.content_part.added", "item_id": "msg_1", "output_index": 1, "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}}),
            json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "Hel"}),
            json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "lo"}),
            json!({"type": "response.output_text.done", "item_id": "msg_1", "output_index": 1, "content_index": 0, "text": "Hello"}),
            json!({"type": "response.content_part.done", "item_id": "msg_1", "output_index": 1, "content_index": 0, "part": {"type": "output_text", "text": "Hello", "annotations": []}}),
            json!({"type": "response.output_item.done", "output_index": 1, "item": completed_output[1]}),
            json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed", "output": completed_output, "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}}}),
        ]);

        let body = a.into_body();
        assert_eq!(body.additional_properties["id"], json!("resp_1"));
        assert_eq!(body.additional_properties["status"], json!("completed"));
        assert_eq!(body.usage.flatten().unwrap().output_tokens, Some(5));
        assert_eq!(
            serde_json::to_value(&body.output).unwrap(),
            completed_output
        );
    }

    #[test]
    fn deltas_alone_build_the_items() {
        let a = assemble(vec![
            json!({"type": "response.output_item.added", "output_index": 0, "item": {"type": "reasoning", "id": "rs_1", "summary": []}}),
            json!({"type": "response.reasoning_text.delta", "item_id": "rs_1", "output_index": 0, "content_index": 0, "delta": "raw "}),
            json!({"type": "response.reasoning_text.delta", "item_id": "rs_1", "output_index": 0, "content_index": 0, "delta": "thought"}),
            json!({"type": "response.output_item.added", "output_index": 1, "item": {"type": "message", "id": "msg_1", "role": "assistant", "content": []}}),
            json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "Hi"}),
            json!({"type": "response.output_item.added", "output_index": 2, "item": {"type": "function_call", "id": "fc_1", "call_id": "c1", "name": "f", "arguments": ""}}),
            json!({"type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 2, "delta": "{\"a\":"}),
            json!({"type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 2, "delta": "1}"}),
        ]);

        let output = a.output();
        let OutputItem::Reasoning { content, .. } = &output[0] else {
            panic!("reasoning")
        };
        assert_eq!(
            content.as_ref().unwrap()[0],
            ReasoningPart::reasoning_text("raw thought")
        );
        let OutputItem::Message { content, .. } = &output[1] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("Hi"));
        let OutputItem::FunctionCall { arguments, .. } = &output[2] else {
            panic!("function_call")
        };
        assert_eq!(arguments, "{\"a\":1}");
    }

    #[test]
    fn deltas_without_item_added_create_the_items() {
        let a = assemble(vec![
            json!({"type": "response.reasoning_summary_text.delta", "output_index": 0, "summary_index": 0, "delta": "s"}),
            json!({"type": "response.output_text.delta", "output_index": 1, "content_index": 0, "delta": "t"}),
        ]);
        let output = a.output();
        let OutputItem::Reasoning { summary, .. } = &output[0] else {
            panic!("reasoning")
        };
        assert_eq!(summary[0], ReasoningPart::summary_text("s"));
        let OutputItem::Message { content, role, .. } = &output[1] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("t"));
        assert_eq!(role.as_deref(), Some("assistant"));
    }

    #[test]
    fn terminal_event_with_empty_output_keeps_assembled_items_and_records_errors() {
        let a = assemble(vec![
            json!({"type": "response.output_text.delta", "output_index": 0, "delta": "partial"}),
            json!({"type": "error", "code": "server_error", "message": "boom"}),
            json!({"type": "response.failed", "response": {"id": "r", "status": "failed", "output": [], "error": {"code": "server_error", "message": "boom"}}}),
        ]);

        assert_eq!(a.error().unwrap()["message"], json!("boom"));
        let body = a.into_body();
        assert_eq!(body.additional_properties["status"], json!("failed"));
        assert_eq!(body.output.len(), 1);
    }
}
