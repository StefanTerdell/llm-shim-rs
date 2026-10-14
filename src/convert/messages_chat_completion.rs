use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::apis::{
    chat_completion::models::api::{
        common::{
            ChatCompletionRequestMessageFunctionToolCall, ChatCompletionRequestMessageToolCall,
            ChatCompletionUsage, CommonChatCompletionMessage,
        },
        request::{
            ChatCompletionRequestBody,
            common::{
                ChatCompletionRequestMessage, ChatCompletionRequestMessageContent,
                ChatCompletionRequestMessageContentPart, CommonChatCompletionRequestBody,
            },
            non_streaming::NonStreamingChatCompletionRequestBody,
            streaming::StreamingChatCompletionRequestBody,
        },
        response::non_streaming::NonStreamingChatCompletionResponseBody,
    },
    messages::models::api::{
        common::{ContentBlock, MessagesUsage},
        request::{
            MessagesRequestBody,
            common::{CommonMessagesRequestBody, MessageContent, MessageParam},
            non_streaming::NonStreamingMessagesRequestBody,
            streaming::StreamingMessagesRequestBody,
        },
        response::non_streaming::MessagesResponseBody,
    },
};

impl From<MessageContent> for ChatCompletionRequestMessageContent {
    fn from(value: MessageContent) -> Self {
        match value {
            MessageContent::Text(text) => Self::Text(text),
            MessageContent::Blocks(blocks) => Self::Parts(
                blocks
                    .into_iter()
                    .map(ChatCompletionRequestMessageContentPart::from)
                    .collect(),
            ),
        }
    }
}

impl From<ContentBlock> for ChatCompletionRequestMessageContentPart {
    fn from(value: ContentBlock) -> Self {
        match value {
            ContentBlock::Text {
                text,
                additional_properties,
            } => Self::Text {
                r#type: Default::default(),
                text,
                additional_properties,
            },
            other => Self::Other(serde_json::to_value(other).unwrap_or(Value::Null)),
        }
    }
}

impl From<MessageParam> for ChatCompletionRequestMessage {
    fn from(value: MessageParam) -> Self {
        let MessageParam {
            content,
            additional_properties,
        } = value;

        let mut common = CommonChatCompletionMessage {
            additional_properties,
            ..Default::default()
        };

        let content = match content {
            MessageContent::Text(text) => Some(ChatCompletionRequestMessageContent::Text(text)),
            MessageContent::Blocks(blocks) => {
                let mut parts = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Thinking { thinking, .. } => {
                            common.reasoning_content_or_default().push_str(&thinking);
                        }
                        ContentBlock::ToolUse {
                            name,
                            input,
                            additional_properties,
                        } => {
                            common.tool_calls.get_or_insert_default().push(
                                ChatCompletionRequestMessageToolCall {
                                    index: None,
                                    function: ChatCompletionRequestMessageFunctionToolCall {
                                        name: Some(name),
                                        arguments: serde_json::to_string(&input).ok(),
                                        additional_properties: Default::default(),
                                    },
                                    additional_properties,
                                },
                            );
                        }
                        block => parts.push(block.into()),
                    }
                }

                (!parts.is_empty()).then_some(ChatCompletionRequestMessageContent::Parts(parts))
            }
        };

        Self { content, common }
    }
}

fn system_message(content: MessageContent) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage {
        content: Some(content.into()),
        common: CommonChatCompletionMessage {
            additional_properties: IndexMap::from([("role".to_string(), json!("system"))]),
            ..Default::default()
        },
    }
}

fn anthropic_tool_to_openai(tool: Value) -> Value {
    let Value::Object(mut tool) = tool else {
        return tool;
    };

    if !tool.contains_key("input_schema") {
        return Value::Object(tool);
    }

    let mut function = serde_json::Map::new();

    if let Some(name) = tool.remove("name") {
        function.insert("name".to_string(), name);
    }
    if let Some(description) = tool.remove("description") {
        function.insert("description".to_string(), description);
    }
    if let Some(schema) = tool.remove("input_schema") {
        function.insert("parameters".to_string(), schema);
    }

    function.extend(tool);

    json!({ "type": "function", "function": Value::Object(function) })
}

fn anthropic_tool_choice_to_openai(choice: Value) -> Value {
    let Some(kind) = choice.get("type").and_then(Value::as_str) else {
        return choice;
    };

    match kind {
        "auto" => json!("auto"),
        "any" => json!("required"),
        "none" => json!("none"),
        "tool" => match choice.get("name") {
            Some(name) => json!({"type": "function", "function": {"name": name}}),
            None => json!("required"),
        },
        _ => choice,
    }
}

impl From<CommonMessagesRequestBody> for CommonChatCompletionRequestBody {
    fn from(value: CommonMessagesRequestBody) -> Self {
        let CommonMessagesRequestBody {
            model,
            messages,
            system,
            mut additional_properties,
        } = value;

        let messages = system
            .map(system_message)
            .into_iter()
            .chain(messages.into_iter().map(ChatCompletionRequestMessage::from))
            .collect();

        if let Some(stop_sequences) = additional_properties.shift_remove("stop_sequences") {
            additional_properties.insert("stop".to_string(), stop_sequences);
        }

        if let Some(Value::Array(tools)) = additional_properties.shift_remove("tools") {
            additional_properties.insert(
                "tools".to_string(),
                Value::Array(tools.into_iter().map(anthropic_tool_to_openai).collect()),
            );
        }

        if let Some(tool_choice) = additional_properties.shift_remove("tool_choice") {
            additional_properties.insert(
                "tool_choice".to_string(),
                anthropic_tool_choice_to_openai(tool_choice),
            );
        }

        Self {
            model,
            messages,
            logprobs: None,
            additional_properties,
        }
    }
}

impl From<NonStreamingMessagesRequestBody> for NonStreamingChatCompletionRequestBody {
    fn from(value: NonStreamingMessagesRequestBody) -> Self {
        Self {
            stream: value.stream,
            common: value.common.into(),
        }
    }
}

impl From<StreamingMessagesRequestBody> for StreamingChatCompletionRequestBody {
    fn from(value: StreamingMessagesRequestBody) -> Self {
        Self {
            stream: value.stream,
            stream_options: None,
            common: value.common.into(),
        }
    }
}

impl From<MessagesRequestBody> for ChatCompletionRequestBody {
    fn from(value: MessagesRequestBody) -> Self {
        match value {
            MessagesRequestBody::NonStreaming(body) => Self::NonStreaming(body.into()),
            MessagesRequestBody::Streaming(body) => Self::Streaming(body.into()),
        }
    }
}

pub(crate) fn finish_reason_to_stop_reason(finish_reason: &str) -> &'static str {
    match finish_reason {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        "content_filter" => "refusal",
        _ => "end_turn",
    }
}

impl From<&ChatCompletionUsage> for MessagesUsage {
    fn from(value: &ChatCompletionUsage) -> Self {
        Self {
            input_tokens: Some(value.prompt_tokens()),
            output_tokens: Some(value.completion_tokens()),
            additional_properties: Default::default(),
        }
    }
}

impl From<NonStreamingChatCompletionResponseBody> for MessagesResponseBody {
    fn from(value: NonStreamingChatCompletionResponseBody) -> Self {
        let NonStreamingChatCompletionResponseBody {
            object: _,
            model,
            choices,
            usage,
            error,
            mut additional_properties,
        } = value;

        let usage = usage.as_ref().map(MessagesUsage::from).unwrap_or_default();

        let mut content = Vec::new();
        let mut stop_reason = None;

        if let Some(choice) = choices.into_iter().next() {
            stop_reason = choice
                .common
                .additional_properties
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(|reason| finish_reason_to_stop_reason(reason).to_string());

            let message = choice.message;

            if let Some(thinking) = message
                .common
                .reasoning_content()
                .or_else(|| message.common.reasoning())
                .filter(|thinking| !thinking.is_empty())
            {
                content.push(ContentBlock::thinking(thinking, ""));
            }

            if let Some(text) = message.content().filter(|text| !text.is_empty()) {
                content.push(ContentBlock::text(text));
            }

            for call in message.common.tool_calls.into_iter().flatten() {
                let mut additional_properties = call.additional_properties;
                additional_properties.shift_remove("type");

                content.push(ContentBlock::ToolUse {
                    name: call.function.name.unwrap_or_default(),
                    input: call
                        .function
                        .arguments
                        .as_deref()
                        .and_then(|arguments| serde_json::from_str(arguments).ok())
                        .unwrap_or_else(|| json!({})),
                    additional_properties,
                });
            }
        }

        additional_properties.insert("type".to_string(), json!("message"));
        additional_properties.insert("role".to_string(), json!("assistant"));

        if let Some(model) = model {
            additional_properties.insert("model".to_string(), json!(model));
        }

        if let Some(error) = error {
            additional_properties.insert("error".to_string(), error);
        }

        Self {
            content,
            stop_reason,
            stop_sequence: None,
            usage,
            additional_properties,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_chat_completion(body: Value) -> Value {
        let parsed: MessagesRequestBody = serde_json::from_value(body).unwrap();
        serde_json::to_value(ChatCompletionRequestBody::from(parsed)).unwrap()
    }

    fn to_messages(body: Value) -> Value {
        let parsed: NonStreamingChatCompletionResponseBody = serde_json::from_value(body).unwrap();
        serde_json::to_value(MessagesResponseBody::from(parsed)).unwrap()
    }

    #[test]
    fn a_system_prompt_becomes_the_first_message() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "max_tokens": 10,
            "system": "be brief",
            "messages": [{"role": "user", "content": "hi"}],
        }));

        assert_eq!(
            converted["messages"],
            json!([
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ])
        );
        assert_eq!(
            converted["max_tokens"],
            json!(10),
            "fields the two formats share must pass straight through"
        );
    }

    #[test]
    fn text_blocks_become_content_parts() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "first"},
                    {"type": "text", "text": "second"},
                ],
            }],
        }));

        assert_eq!(
            converted["messages"][0]["content"],
            json!([
                {"type": "text", "text": "first"},
                {"type": "text", "text": "second"},
            ])
        );
    }

    #[test]
    fn thinking_and_tool_use_move_out_of_the_content_array() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "hmm", "signature": "sig"},
                    {"type": "text", "text": "the answer"},
                    {"type": "tool_use", "id": "tu_1", "name": "lookup", "input": {"q": "x"}},
                ],
            }],
        }));

        let message = &converted["messages"][0];
        assert_eq!(message["reasoning_content"], json!("hmm"));
        assert_eq!(message["content"], json!([{"type": "text", "text": "the answer"}]));
        assert_eq!(message["tool_calls"][0]["function"]["name"], json!("lookup"));
        assert_eq!(
            message["tool_calls"][0]["function"]["arguments"],
            json!("{\"q\":\"x\"}"),
            "openai carries tool arguments as a json string, anthropic as an object"
        );
        assert_eq!(
            message["tool_calls"][0]["id"],
            json!("tu_1"),
            "the call's id must survive so a later tool result can refer to it"
        );
    }

    #[test]
    fn tool_definitions_are_rewritten_into_the_openai_shape() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{
                "name": "lookup",
                "description": "look something up",
                "input_schema": {"type": "object", "properties": {}},
            }],
            "tool_choice": {"type": "any"},
        }));

        assert_eq!(
            converted["tools"],
            json!([{
                "type": "function",
                "function": {
                    "name": "lookup",
                    "description": "look something up",
                    "parameters": {"type": "object", "properties": {}},
                },
            }])
        );
        assert_eq!(converted["tool_choice"], json!("required"));
    }

    #[test]
    fn a_named_tool_choice_keeps_the_name() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "tool_choice": {"type": "tool", "name": "lookup"},
        }));

        assert_eq!(
            converted["tool_choice"],
            json!({"type": "function", "function": {"name": "lookup"}})
        );
    }

    #[test]
    fn stop_sequences_are_renamed() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "stop_sequences": ["END"],
        }));

        assert_eq!(converted["stop"], json!(["END"]));
        assert_eq!(converted.get("stop_sequences"), None);
    }

    #[test]
    fn the_stream_flag_survives_the_conversion() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        }));

        assert_eq!(converted["stream"], json!(true));
    }

    #[test]
    fn a_response_becomes_content_blocks() {
        let converted = to_messages(json!({
            "object": "chat.completion",
            "model": "m",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "the answer", "reasoning_content": "hmm"},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 7, "total_tokens": 10},
        }));

        assert_eq!(
            converted["content"],
            json!([
                {"type": "thinking", "thinking": "hmm", "signature": ""},
                {"type": "text", "text": "the answer"},
            ])
        );
        assert_eq!(converted["stop_reason"], json!("end_turn"));
        assert_eq!(converted["usage"], json!({"input_tokens": 3, "output_tokens": 7}));
        assert_eq!(converted["role"], json!("assistant"));
        assert_eq!(converted["type"], json!("message"));
        assert_eq!(converted["model"], json!("m"));
    }

    #[test]
    fn tool_calls_come_back_as_tool_use_blocks() {
        let converted = to_messages(json!({
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"},
                    }],
                },
                "finish_reason": "tool_calls",
            }],
        }));

        assert_eq!(converted["content"][0]["type"], json!("tool_use"));
        assert_eq!(converted["content"][0]["name"], json!("lookup"));
        assert_eq!(
            converted["content"][0]["input"],
            json!({"q": "x"}),
            "anthropic wants the arguments parsed back into an object"
        );
        assert_eq!(converted["content"][0]["id"], json!("call_1"));
        assert_eq!(converted["stop_reason"], json!("tool_use"));
    }

    #[test]
    fn each_finish_reason_maps_to_its_anthropic_name() {
        for (finish_reason, stop_reason) in [
            ("stop", "end_turn"),
            ("length", "max_tokens"),
            ("tool_calls", "tool_use"),
            ("content_filter", "refusal"),
        ] {
            let converted = to_messages(json!({
                "object": "chat.completion",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "x"},
                    "finish_reason": finish_reason,
                }],
            }));

            assert_eq!(converted["stop_reason"], json!(stop_reason));
        }
    }

    #[test]
    fn an_empty_response_still_carries_the_envelope() {
        let converted = to_messages(json!({"object": "chat.completion", "choices": []}));

        assert_eq!(converted["content"], json!([]));
        assert_eq!(converted["stop_reason"], Value::Null);
        assert_eq!(converted["usage"], json!({}));
    }
}
