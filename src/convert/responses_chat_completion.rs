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
    responses::models::api::{
        common::{ContentPart, OutputItem, ReasoningPart, ResponsesUsage},
        request::{
            ResponsesRequestBody,
            common::{CommonResponsesRequestBody, ResponsesInput},
            non_streaming::NonStreamingResponsesRequestBody,
            streaming::StreamingResponsesRequestBody,
        },
        response::non_streaming::ResponseBody,
    },
};

fn message(role: &str, content: ChatCompletionRequestMessageContent) -> ChatCompletionRequestMessage {
    ChatCompletionRequestMessage {
        content: Some(content),
        common: CommonChatCompletionMessage {
            additional_properties: IndexMap::from([("role".to_string(), json!(role))]),
            ..Default::default()
        },
    }
}

fn content_part(part: Value) -> ChatCompletionRequestMessageContentPart {
    let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();

    match kind {
        "input_text" | "output_text" | "text" => {
            let text = part
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            ChatCompletionRequestMessageContentPart::Text {
                r#type: Default::default(),
                text,
                additional_properties: Default::default(),
            }
        }
        "input_image" => match part.get("image_url") {
            Some(Value::String(url)) => ChatCompletionRequestMessageContentPart::Other(
                json!({"type": "image_url", "image_url": {"url": url}}),
            ),
            _ => ChatCompletionRequestMessageContentPart::Other(part),
        },
        _ => ChatCompletionRequestMessageContentPart::Other(part),
    }
}

fn content(value: Value) -> ChatCompletionRequestMessageContent {
    match value {
        Value::String(text) => ChatCompletionRequestMessageContent::Text(text),
        Value::Array(parts) => {
            ChatCompletionRequestMessageContent::Parts(parts.into_iter().map(content_part).collect())
        }
        other => ChatCompletionRequestMessageContent::Parts(vec![
            ChatCompletionRequestMessageContentPart::Other(other),
        ]),
    }
}

fn input_item(mut item: Value) -> Option<ChatCompletionRequestMessage> {
    let kind = item.get("type").and_then(Value::as_str).unwrap_or("message");

    match kind {
        "function_call" => {
            let name = item.get("name").and_then(Value::as_str)?.to_string();
            let arguments = item
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let call_id = item.get("call_id").or_else(|| item.get("id")).cloned();

            let mut additional_properties =
                IndexMap::from([("role".to_string(), json!("assistant"))]);

            let mut call_properties = IndexMap::from([("type".to_string(), json!("function"))]);
            if let Some(call_id) = call_id {
                call_properties.insert("id".to_string(), call_id);
            }

            Some(ChatCompletionRequestMessage {
                content: None,
                common: CommonChatCompletionMessage {
                    tool_calls: Some(vec![ChatCompletionRequestMessageToolCall {
                        index: None,
                        function: ChatCompletionRequestMessageFunctionToolCall {
                            name: Some(name),
                            arguments: Some(arguments),
                            additional_properties: Default::default(),
                        },
                        additional_properties: call_properties,
                    }]),
                    additional_properties: {
                        additional_properties.shift_remove("type");
                        additional_properties
                    },
                    ..Default::default()
                },
            })
        }
        "function_call_output" => {
            let output = item.get("output").cloned().unwrap_or(Value::Null);
            let call_id = item.get("call_id").cloned();

            let mut additional_properties = IndexMap::from([("role".to_string(), json!("tool"))]);
            if let Some(call_id) = call_id {
                additional_properties.insert("tool_call_id".to_string(), call_id);
            }

            Some(ChatCompletionRequestMessage {
                content: Some(match output {
                    Value::String(text) => ChatCompletionRequestMessageContent::Text(text),
                    other => ChatCompletionRequestMessageContent::Text(other.to_string()),
                }),
                common: CommonChatCompletionMessage {
                    additional_properties,
                    ..Default::default()
                },
            })
        }
        _ => {
            let role = item
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_string();

            let body = item.get_mut("content")?.take();

            Some(message(&role, content(body)))
        }
    }
}

fn responses_tool_to_openai(tool: Value) -> Value {
    let Value::Object(mut tool) = tool else {
        return tool;
    };

    if tool.contains_key("function") || !tool.contains_key("name") {
        return Value::Object(tool);
    }

    let mut function = serde_json::Map::new();
    tool.remove("type");

    if let Some(name) = tool.remove("name") {
        function.insert("name".to_string(), name);
    }
    if let Some(description) = tool.remove("description") {
        function.insert("description".to_string(), description);
    }
    if let Some(parameters) = tool.remove("parameters") {
        function.insert("parameters".to_string(), parameters);
    }

    function.extend(tool);

    json!({ "type": "function", "function": Value::Object(function) })
}

fn responses_tool_choice_to_openai(choice: Value) -> Value {
    match choice.get("type").and_then(Value::as_str) {
        Some("function") => match choice.get("name") {
            Some(name) => json!({"type": "function", "function": {"name": name}}),
            None => choice,
        },
        _ => choice,
    }
}

impl From<CommonResponsesRequestBody> for CommonChatCompletionRequestBody {
    fn from(value: CommonResponsesRequestBody) -> Self {
        let CommonResponsesRequestBody {
            model,
            input,
            instructions,
            mut additional_properties,
        } = value;

        let instruction_message = instructions
            .map(|text| message("system", ChatCompletionRequestMessageContent::Text(text)));

        let input_messages = match input {
            ResponsesInput::Text(text) => {
                vec![message("user", ChatCompletionRequestMessageContent::Text(text))]
            }
            ResponsesInput::Items(items) => items.into_iter().filter_map(input_item).collect(),
        };

        let messages = instruction_message
            .into_iter()
            .chain(input_messages)
            .collect();

        if let Some(max_output_tokens) = additional_properties.shift_remove("max_output_tokens") {
            additional_properties.insert("max_completion_tokens".to_string(), max_output_tokens);
        }

        if let Some(effort) = additional_properties
            .shift_remove("reasoning")
            .and_then(|reasoning| reasoning.get("effort").cloned())
        {
            additional_properties.insert("reasoning_effort".to_string(), effort);
        }

        if let Some(format) = additional_properties
            .shift_remove("text")
            .and_then(|text| text.get("format").cloned())
        {
            additional_properties.insert("response_format".to_string(), format);
        }

        if let Some(Value::Array(tools)) = additional_properties.shift_remove("tools") {
            additional_properties.insert(
                "tools".to_string(),
                Value::Array(tools.into_iter().map(responses_tool_to_openai).collect()),
            );
        }

        if let Some(tool_choice) = additional_properties.shift_remove("tool_choice") {
            additional_properties.insert(
                "tool_choice".to_string(),
                responses_tool_choice_to_openai(tool_choice),
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

impl From<NonStreamingResponsesRequestBody> for NonStreamingChatCompletionRequestBody {
    fn from(value: NonStreamingResponsesRequestBody) -> Self {
        Self {
            stream: value.stream,
            common: value.common.into(),
        }
    }
}

impl From<StreamingResponsesRequestBody> for StreamingChatCompletionRequestBody {
    fn from(value: StreamingResponsesRequestBody) -> Self {
        Self {
            stream: value.stream,
            stream_options: None,
            common: value.common.into(),
        }
    }
}

impl From<ResponsesRequestBody> for ChatCompletionRequestBody {
    fn from(value: ResponsesRequestBody) -> Self {
        match value {
            ResponsesRequestBody::NonStreaming(body) => Self::NonStreaming(body.into()),
            ResponsesRequestBody::Streaming(body) => Self::Streaming(body.into()),
        }
    }
}

impl From<&ChatCompletionUsage> for ResponsesUsage {
    fn from(value: &ChatCompletionUsage) -> Self {
        Self {
            input_tokens: Some(value.prompt_tokens()),
            output_tokens: Some(value.completion_tokens()),
            total_tokens: Some(value.total_tokens()),
            additional_properties: Default::default(),
        }
    }
}

impl From<NonStreamingChatCompletionResponseBody> for ResponseBody {
    fn from(value: NonStreamingChatCompletionResponseBody) -> Self {
        let NonStreamingChatCompletionResponseBody {
            object: _,
            model,
            choices,
            usage,
            error,
            mut additional_properties,
        } = value;

        let usage = usage.as_ref().map(ResponsesUsage::from);

        let mut output = Vec::new();

        if let Some(choice) = choices.into_iter().next() {
            let message = choice.message;

            if let Some(reasoning) = message
                .common
                .reasoning_content()
                .or_else(|| message.common.reasoning())
                .filter(|reasoning| !reasoning.is_empty())
            {
                output.push(OutputItem::Reasoning {
                    id: None,
                    summary: vec![],
                    content: Some(vec![ReasoningPart::reasoning_text(reasoning)]),
                    additional_properties: Default::default(),
                });
            }

            if let Some(text) = message.content().filter(|text| !text.is_empty()) {
                output.push(OutputItem::Message {
                    id: None,
                    role: Some("assistant".to_string()),
                    status: Some("completed".to_string()),
                    content: vec![ContentPart::output_text(text)],
                    additional_properties: Default::default(),
                });
            }

            for call in message.common.tool_calls.into_iter().flatten() {
                let mut call_properties = call.additional_properties;
                call_properties.shift_remove("type");

                if let Some(id) = call_properties.shift_remove("id") {
                    call_properties.insert("call_id".to_string(), id);
                }

                output.push(OutputItem::FunctionCall {
                    id: None,
                    name: call.function.name.unwrap_or_default(),
                    arguments: call.function.arguments.unwrap_or_default(),
                    additional_properties: call_properties,
                });
            }
        }

        additional_properties.insert("object".to_string(), json!("response"));
        additional_properties.insert("status".to_string(), json!("completed"));

        if let Some(model) = model {
            additional_properties.insert("model".to_string(), json!(model));
        }

        Self {
            output,
            usage: Some(usage),
            error: Some(error),
            additional_properties,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_chat_completion(body: Value) -> Value {
        let parsed: ResponsesRequestBody = serde_json::from_value(body).unwrap();
        serde_json::to_value(ChatCompletionRequestBody::from(parsed)).unwrap()
    }

    fn to_responses(body: Value) -> Value {
        let parsed: NonStreamingChatCompletionResponseBody = serde_json::from_value(body).unwrap();
        serde_json::to_value(ResponseBody::from(parsed)).unwrap()
    }

    #[test]
    fn plain_text_input_becomes_a_user_message() {
        let converted = to_chat_completion(json!({"model": "m", "input": "hello"}));

        assert_eq!(
            converted["messages"],
            json!([{"role": "user", "content": "hello"}])
        );
    }

    #[test]
    fn instructions_become_a_system_message() {
        let converted =
            to_chat_completion(json!({"model": "m", "input": "hi", "instructions": "be brief"}));

        assert_eq!(
            converted["messages"],
            json!([
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ])
        );
    }

    #[test]
    fn input_parts_are_renamed_to_the_chat_completions_shape() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "input": [{
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "look at this"},
                    {"type": "input_image", "image_url": "https://example.test/a.png"},
                ],
            }],
        }));

        assert_eq!(
            converted["messages"][0]["content"],
            json!([
                {"type": "text", "text": "look at this"},
                {"type": "image_url", "image_url": {"url": "https://example.test/a.png"}},
            ])
        );
    }

    #[test]
    fn a_function_call_and_its_output_become_a_tool_exchange() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "input": [
                {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"x\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "the result"},
            ],
        }));

        let call = &converted["messages"][0];
        assert_eq!(call["role"], json!("assistant"));
        assert_eq!(call["tool_calls"][0]["id"], json!("call_1"));
        assert_eq!(call["tool_calls"][0]["type"], json!("function"));
        assert_eq!(call["tool_calls"][0]["function"]["name"], json!("lookup"));

        let result = &converted["messages"][1];
        assert_eq!(result["role"], json!("tool"));
        assert_eq!(result["tool_call_id"], json!("call_1"));
        assert_eq!(result["content"], json!("the result"));
    }

    #[test]
    fn tools_are_nested_under_function() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "input": "hi",
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "look it up",
                "parameters": {"type": "object"},
            }],
            "tool_choice": {"type": "function", "name": "lookup"},
        }));

        assert_eq!(
            converted["tools"],
            json!([{
                "type": "function",
                "function": {
                    "name": "lookup",
                    "description": "look it up",
                    "parameters": {"type": "object"},
                },
            }])
        );
        assert_eq!(
            converted["tool_choice"],
            json!({"type": "function", "function": {"name": "lookup"}})
        );
    }

    #[test]
    fn the_knobs_the_two_formats_name_differently_are_renamed() {
        let converted = to_chat_completion(json!({
            "model": "m",
            "input": "hi",
            "max_output_tokens": 256,
            "reasoning": {"effort": "high"},
            "text": {"format": {"type": "json_object"}},
        }));

        assert_eq!(converted["max_completion_tokens"], json!(256));
        assert_eq!(converted["reasoning_effort"], json!("high"));
        assert_eq!(converted["response_format"], json!({"type": "json_object"}));
        assert_eq!(converted.get("max_output_tokens"), None);
    }

    #[test]
    fn the_stream_flag_survives_the_conversion() {
        let converted = to_chat_completion(json!({"model": "m", "input": "hi", "stream": true}));

        assert_eq!(converted["stream"], json!(true));
    }

    #[test]
    fn a_response_becomes_output_items() {
        let converted = to_responses(json!({
            "object": "chat.completion",
            "model": "m",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "the answer", "reasoning_content": "hmm"},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 7, "total_tokens": 10},
        }));

        assert_eq!(converted["output"][0]["type"], json!("reasoning"));
        assert_eq!(
            converted["output"][0]["content"],
            json!([{"type": "reasoning_text", "text": "hmm"}])
        );
        assert_eq!(converted["output"][1]["type"], json!("message"));
        assert_eq!(
            converted["output"][1]["content"],
            json!([{"type": "output_text", "text": "the answer", "annotations": []}])
        );
        assert_eq!(
            converted["usage"],
            json!({"input_tokens": 3, "output_tokens": 7, "total_tokens": 10})
        );
        assert_eq!(converted["object"], json!("response"));
        assert_eq!(converted["status"], json!("completed"));
    }

    #[test]
    fn tool_calls_come_back_as_function_call_items() {
        let converted = to_responses(json!({
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

        assert_eq!(converted["output"][0]["type"], json!("function_call"));
        assert_eq!(converted["output"][0]["name"], json!("lookup"));
        assert_eq!(
            converted["output"][0]["arguments"],
            json!("{\"q\":\"x\"}"),
            "responses keeps the arguments as a string, like chat completions does"
        );
        assert_eq!(
            converted["output"][0]["call_id"],
            json!("call_1"),
            "responses names it call_id, so a later function_call_output can refer to it"
        );
    }

    #[test]
    fn an_empty_response_still_carries_the_envelope() {
        let converted = to_responses(json!({"object": "chat.completion", "choices": []}));

        assert_eq!(converted["output"], json!([]));
        assert_eq!(converted["usage"], Value::Null);
        assert_eq!(converted["error"], Value::Null);
        assert_eq!(converted["object"], json!("response"));
    }
}
