use arrayvec::ArrayString;
use indexmap::IndexMap;

use crate::chat_completion::models::api::response::common::ChatCompletionResponseMessage;

#[derive(..ApiModel, Copy, Eq, Hash)]
pub struct ChatCompletionReasoningRemappingConfig {
    pub from: ChatCompletionReasoningPosition,
    pub to: ChatCompletionReasoningPosition,
}

impl ChatCompletionReasoningRemappingConfig {
    pub fn into_state(self) -> ChatCompletionReasoningRemappingState {
        ChatCompletionReasoningRemappingState::new(self)
    }
}

impl
    From<(
        ChatCompletionReasoningPosition,
        ChatCompletionReasoningPosition,
    )> for ChatCompletionReasoningRemappingConfig
{
    fn from(
        (from, to): (
            ChatCompletionReasoningPosition,
            ChatCompletionReasoningPosition,
        ),
    ) -> Self {
        Self { from, to }
    }
}

#[derive(..ApiModel, Copy, Eq, Hash)]
pub enum ChatCompletionReasoningPosition {
    Content {
        start_tag: ArrayString<32>,
        stop_tag: ArrayString<32>,
    },
    ReasoningContent,
    Reasoning,
}

impl ChatCompletionReasoningPosition {
    pub fn content(
        start_tag: impl AsRef<str>,
        stop_tag: impl AsRef<str>,
    ) -> Result<Self, &'static str> {
        Ok(Self::Content {
            start_tag: ArrayString::from(start_tag.as_ref())
                .map_err(|_| "start_tag length must be lte 32")?,
            stop_tag: ArrayString::from(stop_tag.as_ref())
                .map_err(|_| "stop_tag length must be lte 32")?,
        })
    }

    pub fn content_unchecked(start_tag: impl AsRef<str>, stop_tag: impl AsRef<str>) -> Self {
        Self::content(start_tag, stop_tag).unwrap()
    }
}

pub struct ChatCompletionReasoningRemappingState {
    from: ChatCompletionReasoningPosition,
    to: ChatCompletionReasoningPosition,
    choices: IndexMap<usize, ChoiceState>,
}

#[derive(Default, Clone)]
struct ChoiceState {
    started: bool,
    stopped: bool,
    partial: Option<String>,
}

impl ChatCompletionReasoningRemappingState {
    pub fn new(positions: impl Into<ChatCompletionReasoningRemappingConfig>) -> Self {
        let ChatCompletionReasoningRemappingConfig { from, to } = positions.into();
        Self {
            from,
            to,
            choices: IndexMap::new(),
        }
    }

    pub fn apply(&mut self, index: u32, inner: &mut ChatCompletionResponseMessage) {
        let index = index as usize;

        let state = self.choices.entry(index).or_default();
        let was_started = state.started;
        let was_stopped = state.stopped;

        let mut reasoning_content_opt = None;
        let mut reasoning_offset = 0;

        match (state.started, state.stopped) {
            // not started, not stopped
            (false, false) => match &self.from {
                ChatCompletionReasoningPosition::Content {
                    start_tag,
                    stop_tag,
                } => {
                    let Some(content) = inner.content.as_mut() else {
                        return;
                    };

                    if let Some(partial_start_tag) = state.partial.take() {
                        *content = format!("{partial_start_tag}{content}");
                    }

                    if let Some(start_tag_index) = content.find(start_tag.as_str()) {
                        reasoning_offset = start_tag_index;
                        state.started = true;
                        let reasoning_start_index = start_tag_index + start_tag.len();

                        if let Some(reasoning_stop_index) = content.find(stop_tag.as_str())
                            && reasoning_stop_index > reasoning_start_index
                        {
                            state.stopped = true;
                            let content_start_index = reasoning_stop_index + stop_tag.len();

                            reasoning_content_opt = Some(
                                content[reasoning_start_index..reasoning_stop_index].to_string(),
                            );

                            let before = &content[..start_tag_index];
                            let after = &content[content_start_index..];

                            *content = format!("{before}{after}");
                        } else {
                            reasoning_content_opt = Some(content.split_off(reasoning_start_index));
                            content.truncate(start_tag_index);
                        }
                    } else {
                        for max in 1..start_tag.len() {
                            let partial_start_tag = &start_tag[..max];
                            if content.ends_with(partial_start_tag) {
                                state.partial = Some(partial_start_tag.to_string());
                                content.truncate(content.len() - max);
                                break;
                            }
                        }
                    }
                }
                ChatCompletionReasoningPosition::Reasoning => {
                    reasoning_content_opt = inner.common.reasoning.take();
                    state.started = reasoning_content_opt.is_some();
                }
                ChatCompletionReasoningPosition::ReasoningContent => {
                    reasoning_content_opt = inner.common.reasoning_content.take();
                    state.started = reasoning_content_opt.is_some();
                }
            },
            // started, not stopped
            (true, false) => match &self.from {
                ChatCompletionReasoningPosition::Content {
                    start_tag: _,
                    stop_tag,
                } => {
                    let Some(content) = inner.content.as_mut() else {
                        return;
                    };

                    if let Some(partial_stop_tag) = state.partial.take() {
                        *content = format!("{partial_stop_tag}{content}");
                    }

                    if let Some(stop_index) = content.find(stop_tag.as_str()) {
                        state.stopped = true;

                        reasoning_content_opt = Some(content[..stop_index].to_string());
                        *content = content[stop_index + stop_tag.len()..].to_string();
                    } else {
                        for max in 1..stop_tag.len() {
                            let partial_stop_tag = &stop_tag[..max];
                            if content.ends_with(partial_stop_tag) {
                                state.partial = Some(partial_stop_tag.to_string());
                                content.truncate(content.len() - max);
                                break;
                            }
                        }

                        reasoning_content_opt = Some(std::mem::take(content));
                    }
                }
                ChatCompletionReasoningPosition::Reasoning => {
                    reasoning_content_opt = inner.common.reasoning.take();
                    state.stopped = reasoning_content_opt.is_none();
                }
                ChatCompletionReasoningPosition::ReasoningContent => {
                    reasoning_content_opt = inner.common.reasoning_content.take();
                    state.stopped = reasoning_content_opt.is_none();
                }
            },
            // started, stopped
            (true, true) => {
                // no-op
            }
            // not started, stopped
            (false, true) => {
                // wtf
            }
        }

        match &self.to {
            ChatCompletionReasoningPosition::Content {
                start_tag,
                stop_tag,
            } => {
                let content_opt = &mut inner.content;

                if let Some(mut reasoning_content) = reasoning_content_opt {
                    if state.started && !was_started {
                        reasoning_content = format!("{start_tag}{reasoning_content}");
                    }

                    if state.stopped {
                        reasoning_content = format!("{reasoning_content}{stop_tag}")
                    }

                    if let Some(content) = content_opt.as_mut() {
                        let before = &content[..reasoning_offset];
                        let after = &content[reasoning_offset..];

                        *content = format!("{before}{reasoning_content}{after}");
                    } else {
                        *content_opt = Some(reasoning_content)
                    }
                } else if state.stopped && !was_stopped {
                    if let Some(content) = content_opt {
                        *content = format!("{stop_tag}{content}");
                    } else {
                        *content_opt = Some(stop_tag.to_string());
                    }
                }
            }
            ChatCompletionReasoningPosition::Reasoning => {
                if reasoning_content_opt.is_some() {
                    inner.common.reasoning = reasoning_content_opt;
                }
            }
            ChatCompletionReasoningPosition::ReasoningContent => {
                if reasoning_content_opt.is_some() {
                    inner.common.reasoning_content = reasoning_content_opt;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helpers::*;

    #[test]
    fn should_be_able_to_map_between_tags() {
        let mut input = from_content("--<herp>123</derp>asdfg");
        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::content_unchecked("<herp>", "</derp>"),
            ChatCompletionReasoningPosition::content_unchecked("FOO", "BAR"),
        ));

        state.apply(0, &mut input);

        assert_eq!(input.content.unwrap(), "--FOO123BARasdfg");
    }

    #[test]
    fn should_be_able_to_map_between_points_over_chunks() {
        let mut input = [
            from_content("-"),
            from_content("-<herp>1"),
            from_content("2"),
            from_content("3</derp>asd"),
            from_content("fg"),
        ];

        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::content_unchecked("<herp>", "</derp>"),
            ChatCompletionReasoningPosition::ReasoningContent,
        ));

        for chunk in input.iter_mut() {
            state.apply(0, chunk);
        }

        assert_eq!(
            from_chunks(input),
            from_content("--asdfg").with_reasoning_content("123")
        );
    }

    #[test]
    fn should_map_content_tags_to_reasoning_field() {
        let mut input = from_content("before<think>my reasoning</think>after");
        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::content_unchecked("<think>", "</think>"),
            ChatCompletionReasoningPosition::Reasoning,
        ));

        state.apply(0, &mut input);

        assert_eq!(
            input,
            from_content("beforeafter").with_reasoning("my reasoning")
        );
    }

    #[test]
    fn should_map_reasoning_field_to_content_tags() {
        // Streaming: reasoning comes in one chunk, then a content-only chunk
        // signals reasoning is done (reasoning field goes to None → stop tag emitted).
        let mut chunks = [
            from_reasoning("deep thought"),
            from_content("after"), // reasoning is None → triggers stop
        ];

        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::Reasoning,
            ChatCompletionReasoningPosition::content_unchecked("[THINK]", "[/THINK]"),
        ));

        for chunk in chunks.iter_mut() {
            state.apply(0, chunk);
        }

        assert_eq!(
            from_chunks(chunks),
            from_content("[THINK]deep thought[/THINK]after")
        );
    }

    #[test]
    fn should_map_reasoning_content_to_reasoning() {
        let mut input = from_reasoning_content("extracted thought");
        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::ReasoningContent,
            ChatCompletionReasoningPosition::Reasoning,
        ));

        state.apply(0, &mut input);

        assert!(input.common.reasoning_content.is_none());
        assert_eq!(input.common.reasoning.unwrap(), "extracted thought");
    }

    #[test]
    fn should_map_reasoning_to_content_over_chunks() {
        let mut chunks = [
            from_reasoning("part1"),
            from_reasoning("part2"),
            from_content("hello"), // reasoning is None → stop
            from_content("world"),
        ];

        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::Reasoning,
            ChatCompletionReasoningPosition::content_unchecked("<r>", "</r>"),
        ));

        for chunk in chunks.iter_mut() {
            state.apply(0, chunk);
        }

        assert_eq!(
            from_chunks(chunks),
            from_content("<r>part1part2</r>helloworld")
        );
    }

    #[test]
    fn should_handle_any_index() {
        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::content_unchecked("<t>", "</t>"),
            ChatCompletionReasoningPosition::ReasoningContent,
        ));

        for index in [0, 10, 100, 1000] {
            let mut input = from_content("<t>ok</t>");
            state.apply(index, &mut input);
            assert_eq!(input.common.reasoning_content.unwrap(), "ok");
        }
    }

    #[test]
    fn should_buffer_partial_tags_between_chunks() {
        let mut chunks = [
            from_content("me<thin"),
            from_content("k>thoug"),
            from_content("ht</thi"),
            from_content("nk>ssage"),
        ];

        let mut state = ChatCompletionReasoningRemappingState::new((
            ChatCompletionReasoningPosition::content_unchecked("<think>", "</think>"),
            ChatCompletionReasoningPosition::ReasoningContent,
        ));

        for chunk in chunks.iter_mut() {
            state.apply(0, chunk);
        }

        assert_eq!(
            from_chunks(chunks),
            from_content("message").with_reasoning_content("thought")
        );
    }

    mod helpers {
        use super::*;
        use crate::chat_completion::models::api::common::CommonChatCompletionMessage;
        use std::fmt::Display;

        fn message() -> ChatCompletionResponseMessage {
            ChatCompletionResponseMessage {
                content: None,
                common: CommonChatCompletionMessage {
                    tool_calls: None,
                    reasoning_content: None,
                    reasoning: None,
                    additional_properties: Default::default(),
                },
            }
        }

        pub fn from_content(content: impl Display) -> ChatCompletionResponseMessage {
            ChatCompletionResponseMessage {
                content: Some(content.to_string()),
                ..message()
            }
        }

        pub fn from_reasoning(reasoning: impl Display) -> ChatCompletionResponseMessage {
            message().with_reasoning(reasoning)
        }

        pub fn from_reasoning_content(
            reasoning_content: impl Display,
        ) -> ChatCompletionResponseMessage {
            message().with_reasoning_content(reasoning_content)
        }

        pub fn from_chunks(
            chunks: impl IntoIterator<Item = ChatCompletionResponseMessage>,
        ) -> ChatCompletionResponseMessage {
            chunks.into_iter().fold(message(), |prev, curr| prev + curr)
        }

        pub trait With {
            fn with_reasoning(self, reasoning: impl Display) -> Self;
            fn with_reasoning_content(self, reasoning_content: impl Display) -> Self;
        }

        impl With for ChatCompletionResponseMessage {
            fn with_reasoning(mut self, reasoning: impl Display) -> Self {
                self.common.reasoning = Some(reasoning.to_string());
                self
            }

            fn with_reasoning_content(mut self, reasoning_content: impl Display) -> Self {
                self.common.reasoning_content = Some(reasoning_content.to_string());
                self
            }
        }
    }
}
