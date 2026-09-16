use serde_json::Value;

use crate::{
    chat_completion::models::api::{
        common::CommonChatCompletionMessage,
        request::common::{
            ChatCompletionRequestMessage, ChatCompletionRequestMessageContent,
            ChatCompletionRequestMessageContentPart, CommonChatCompletionRequestBody,
        },
        response::common::ChatCompletionResponseMessage,
    },
    messages::models::api::{
        common::{ContentBlock, ContentBlockDelta},
        request::common::{CommonMessagesRequestBody, MessageContent, MessageParam},
    },
    responses::models::api::{
        common::{ContentPart, OutputItem, ReasoningPart},
        request::common::{CommonResponsesRequestBody, ResponsesInput},
    },
};

pub trait EstimateTokens {
    /// Tokenizer-free prompt estimate for fallback accounting when a
    /// stream ends before usage arrives. Deliberately biased high.
    fn estimate_tokens(&self) -> u32;
}

impl EstimateTokens for CommonChatCompletionRequestBody {
    /// Tokenizer-free prompt estimate for fallback accounting when a stream ends
    /// before `usage` arrives. Deliberately biased high. Constants are knobs —
    /// calibrate them (see below).
    fn estimate_tokens(&self) -> u32 {
        const PRIMING_OVERHEAD: u32 = 3; // <|start|>assistant<|message|>

        let mut total = PRIMING_OVERHEAD;

        for msg in &self.messages {
            total += msg.estimate_tokens()
        }

        total
    }
}

impl EstimateTokens for str {
    /// Byte-based, biased high. More stable than word-splitting across code, JSON,
    /// URLs, and CJK. ~4 bytes/token is the English/ASCII average under o200k-style
    /// BPE; a smaller divisor overestimates.
    fn estimate_tokens(&self) -> u32 {
        (self.len() as f64 / 3.5).ceil() as u32 // s.len() is UTF-8 byte length
    }
}

impl EstimateTokens for CommonChatCompletionMessage {
    fn estimate_tokens(&self) -> u32 {
        // ChatML-ish per-message frame: <|im_start|>{role}\n … <|im_end|>\n
        // Harmony (gpt-oss) is heavier with channel markers, so treat this as a floor.
        const PER_MESSAGE_OVERHEAD: u32 = 4;

        let mut total = PER_MESSAGE_OVERHEAD;

        if let Some(role) = &self.role {
            total += role.estimate_tokens();
        };

        if let Some(name) = &self.name {
            total += name.estimate_tokens() + 1;
        }

        // Tool calls cost tokens too: fn name + the arguments JSON string.
        for call in self.tool_calls.iter().flatten() {
            total += call.function.name.estimate_tokens()
                + call.function.arguments.estimate_tokens()
                + 4; // rough per-call structural framing
        }

        total
    }
}

impl EstimateTokens for ChatCompletionResponseMessage {
    fn estimate_tokens(&self) -> u32 {
        self.common.estimate_tokens()
            + self
                .content
                .as_ref()
                .map(|c| c.estimate_tokens())
                .unwrap_or_default()
    }
}

impl EstimateTokens for ChatCompletionRequestMessage {
    fn estimate_tokens(&self) -> u32 {
        const IMAGE_TOKENS: u32 = 1200; // flat, when dimensions/detail unknown
        let total = self.common.estimate_tokens();

        match &self.content {
            Some(ChatCompletionRequestMessageContent::Text(s)) => total + s.estimate_tokens(),
            Some(ChatCompletionRequestMessageContent::Parts(parts)) => {
                parts.iter().fold(total, |total, part| {
                    total
                        + match part {
                            ChatCompletionRequestMessageContentPart::Text { text, .. } => {
                                text.estimate_tokens()
                            }
                            ChatCompletionRequestMessageContentPart::Other(_) => IMAGE_TOKENS, // audio/other modalities: their own flat constant
                        }
                })
            }
            None => total, // e.g. assistant turn that only carries tool_calls
        }
    }
}

impl EstimateTokens for CommonMessagesRequestBody {
    fn estimate_tokens(&self) -> u32 {
        const PRIMING_OVERHEAD: u32 = 3;

        let mut total = PRIMING_OVERHEAD;

        if let Some(system) = &self.system {
            total += system.estimate_tokens();
        }

        for message in &self.messages {
            total += message.estimate_tokens();
        }

        total
    }
}

impl EstimateTokens for MessageParam {
    fn estimate_tokens(&self) -> u32 {
        const PER_MESSAGE_OVERHEAD: u32 = 4;

        PER_MESSAGE_OVERHEAD + self.role.estimate_tokens() + self.content.estimate_tokens()
    }
}

impl EstimateTokens for MessageContent {
    fn estimate_tokens(&self) -> u32 {
        match self {
            MessageContent::Text(text) => text.estimate_tokens(),
            MessageContent::Blocks(blocks) => blocks.iter().map(|b| b.estimate_tokens()).sum(),
        }
    }
}

impl EstimateTokens for ContentBlock {
    fn estimate_tokens(&self) -> u32 {
        const IMAGE_TOKENS: u32 = 1200;

        match self {
            ContentBlock::Text { text, .. } => text.estimate_tokens(),
            ContentBlock::Thinking { thinking, .. } => thinking.estimate_tokens(),
            ContentBlock::RedactedThinking { data, .. } => data.estimate_tokens(),
            ContentBlock::ToolUse { name, input, .. } => {
                name.estimate_tokens() + input.to_string().estimate_tokens() + 4
            }
            ContentBlock::Other(value) => match value.get("type").and_then(Value::as_str) {
                Some("image") | Some("document") => IMAGE_TOKENS,
                _ => value.to_string().estimate_tokens(),
            },
        }
    }
}

impl EstimateTokens for ContentBlockDelta {
    fn estimate_tokens(&self) -> u32 {
        match self {
            ContentBlockDelta::TextDelta { text, .. } => text.estimate_tokens(),
            ContentBlockDelta::ThinkingDelta { thinking, .. } => thinking.estimate_tokens(),
            ContentBlockDelta::InputJsonDelta { partial_json, .. } => {
                partial_json.estimate_tokens()
            }
            ContentBlockDelta::SignatureDelta { .. } => 0,
            ContentBlockDelta::Other(_) => 0,
        }
    }
}

impl EstimateTokens for CommonResponsesRequestBody {
    fn estimate_tokens(&self) -> u32 {
        const PRIMING_OVERHEAD: u32 = 3;

        let mut total = PRIMING_OVERHEAD;

        if let Some(instructions) = &self.instructions {
            total += instructions.estimate_tokens() + 4;
        }

        total += match &self.input {
            ResponsesInput::Text(text) => text.estimate_tokens() + 4,
            ResponsesInput::Items(items) => items
                .iter()
                .map(|item| item.to_string().estimate_tokens() + 4)
                .sum(),
        };

        total
    }
}

impl EstimateTokens for OutputItem {
    fn estimate_tokens(&self) -> u32 {
        match self {
            OutputItem::Message { content, .. } => {
                content.iter().map(EstimateTokens::estimate_tokens).sum()
            }
            OutputItem::Reasoning {
                summary, content, ..
            } => summary
                .iter()
                .chain(content.iter().flatten())
                .map(EstimateTokens::estimate_tokens)
                .sum(),
            OutputItem::FunctionCall {
                name, arguments, ..
            } => name.estimate_tokens() + arguments.estimate_tokens(),
            OutputItem::Other(_) => 0,
        }
    }
}

impl EstimateTokens for ContentPart {
    fn estimate_tokens(&self) -> u32 {
        match self {
            ContentPart::OutputText { text, .. } | ContentPart::ReasoningText { text, .. } => {
                text.estimate_tokens()
            }
            ContentPart::Refusal { refusal, .. } => refusal.estimate_tokens(),
            ContentPart::Other(_) => 0,
        }
    }
}

impl EstimateTokens for ReasoningPart {
    fn estimate_tokens(&self) -> u32 {
        self.text()
            .map(EstimateTokens::estimate_tokens)
            .unwrap_or(0)
    }
}
