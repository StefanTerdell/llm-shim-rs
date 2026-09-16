use arrayvec::ArrayString;
use indexmap::IndexMap;

use crate::traits::reasoning_content_remapping::{
    ReasoningContentRemappingInnerTarget, ReasoningContentRemappingTarget,
};

#[derive(..ApiModel, Copy, Eq, Hash)]
pub struct ReasoningContentRemappingConfig {
    pub from: ReasoningContentPosition,
    pub to: ReasoningContentPosition,
}

impl ReasoningContentRemappingConfig {
    pub fn into_state(self) -> ReasoningContentRemappingState {
        ReasoningContentRemappingState::new(self)
    }
}

impl From<(ReasoningContentPosition, ReasoningContentPosition)>
    for ReasoningContentRemappingConfig
{
    fn from((from, to): (ReasoningContentPosition, ReasoningContentPosition)) -> Self {
        Self { from, to }
    }
}

#[derive(..ApiModel, Copy, Eq, Hash)]
pub enum ReasoningContentPosition {
    Content {
        start_tag: ArrayString<32>,
        stop_tag: ArrayString<32>,
    },
    ReasoningContent,
    Reasoning,
}

impl ReasoningContentPosition {
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

pub struct ReasoningContentRemappingState {
    from: ReasoningContentPosition,
    to: ReasoningContentPosition,
    choices: IndexMap<usize, ChoiceState>,
}

#[derive(Default, Clone)]
struct ChoiceState {
    started: bool,
    stopped: bool,
    partial: Option<String>,
}

impl ReasoningContentRemappingState {
    pub fn new(positions: impl Into<ReasoningContentRemappingConfig>) -> Self {
        let ReasoningContentRemappingConfig { from, to } = positions.into();
        Self {
            from,
            to,
            choices: IndexMap::new(),
        }
    }

    pub fn apply<T: ReasoningContentRemappingTarget>(&mut self, target: &mut T) {
        let index = target.index();

        let Some(inner) = target.inner_mut_opt() else {
            return;
        };

        let state = self.choices.entry(index).or_default();
        let was_started = state.started;
        let was_stopped = state.stopped;

        let mut reasoning_content_opt = None;
        let mut reasoning_offset = 0;

        match (state.started, state.stopped) {
            // not started, not stopped
            (false, false) => match &self.from {
                ReasoningContentPosition::Content {
                    start_tag,
                    stop_tag,
                } => {
                    let Some(content) = inner.content_mut() else {
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
                ReasoningContentPosition::Reasoning => {
                    reasoning_content_opt = inner.reasoning_mut().take();
                    state.started = reasoning_content_opt.is_some();
                }
                ReasoningContentPosition::ReasoningContent => {
                    reasoning_content_opt = inner.reasoning_content_mut().take();
                    state.started = reasoning_content_opt.is_some();
                }
            },
            // started, not stopped
            (true, false) => match &self.from {
                ReasoningContentPosition::Content {
                    start_tag: _,
                    stop_tag,
                } => {
                    let Some(content) = inner.content_mut() else {
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
                ReasoningContentPosition::Reasoning => {
                    reasoning_content_opt = inner.reasoning_mut().take();
                    state.stopped = reasoning_content_opt.is_none();
                }
                ReasoningContentPosition::ReasoningContent => {
                    reasoning_content_opt = inner.reasoning_content_mut().take();
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
            ReasoningContentPosition::Content {
                start_tag,
                stop_tag,
            } => {
                let content_opt = inner.content_mut();

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
            ReasoningContentPosition::Reasoning => {
                if reasoning_content_opt.is_some() {
                    *inner.reasoning_mut() = reasoning_content_opt;
                }
            }
            ReasoningContentPosition::ReasoningContent => {
                if reasoning_content_opt.is_some() {
                    *inner.reasoning_content_mut() = reasoning_content_opt;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dummy::*;

    #[test]
    fn should_be_able_to_map_between_tags() {
        let mut input = Dummy::from_content("--<herp>123</derp>asdfg");
        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::content_unchecked("<herp>", "</derp>"),
            ReasoningContentPosition::content_unchecked("FOO", "BAR"),
        ));

        state.apply(&mut input);

        assert_eq!(input.content.unwrap(), "--FOO123BARasdfg");
    }

    #[test]
    fn should_be_able_to_map_between_points_over_chunks() {
        let mut input = [
            Dummy::from_content("-"),
            Dummy::from_content("-<herp>1"),
            Dummy::from_content("2"),
            Dummy::from_content("3</derp>asd"),
            Dummy::from_content("fg"),
        ];

        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::content_unchecked("<herp>", "</derp>"),
            ReasoningContentPosition::ReasoningContent,
        ));

        for chunk in input.iter_mut() {
            state.apply(chunk);
        }

        assert_eq!(
            Dummy::from_chunks(input),
            Dummy::from_content("--asdfg").with_reasoning_content("123")
        );
    }

    #[test]
    fn should_map_content_tags_to_reasoning_field() {
        let mut input = Dummy::from_content("before<think>my reasoning</think>after");
        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::content_unchecked("<think>", "</think>"),
            ReasoningContentPosition::Reasoning,
        ));

        state.apply(&mut input);

        assert_eq!(
            input,
            Dummy::from_content("beforeafter").with_reasoning("my reasoning")
        );
    }

    #[test]
    fn should_map_reasoning_field_to_content_tags() {
        // Streaming: reasoning comes in one chunk, then a content-only chunk
        // signals reasoning is done (reasoning field goes to None → stop tag emitted).
        let mut chunks = [
            Dummy::from_reasoning("deep thought"),
            Dummy::from_content("after"), // reasoning is None → triggers stop
        ];

        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::Reasoning,
            ReasoningContentPosition::content_unchecked("[THINK]", "[/THINK]"),
        ));

        for chunk in chunks.iter_mut() {
            state.apply(chunk);
        }

        assert_eq!(
            Dummy::from_chunks(chunks),
            Dummy::from_content("[THINK]deep thought[/THINK]after")
        );
    }

    #[test]
    fn should_map_reasoning_content_to_reasoning() {
        let mut input = Dummy::from_reasoning_content("extracted thought");
        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::ReasoningContent,
            ReasoningContentPosition::Reasoning,
        ));

        state.apply(&mut input);

        assert!(input.reasoning_content.is_none());
        assert_eq!(input.reasoning.unwrap(), "extracted thought");
    }

    #[test]
    fn should_map_reasoning_to_content_over_chunks() {
        let mut chunks = [
            Dummy::from_reasoning("part1"),
            Dummy::from_reasoning("part2"),
            Dummy::from_content("hello"), // reasoning is None → stop
            Dummy::from_content("world"),
        ];

        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::Reasoning,
            ReasoningContentPosition::content_unchecked("<r>", "</r>"),
        ));

        for chunk in chunks.iter_mut() {
            state.apply(chunk);
        }

        assert_eq!(
            Dummy::from_chunks(chunks),
            Dummy::from_content("<r>part1part2</r>helloworld")
        );
    }

    #[test]
    fn should_handle_any_index() {
        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::content_unchecked("<t>", "</t>"),
            ReasoningContentPosition::ReasoningContent,
        ));

        for index in [0, 10, 100, 1000] {
            let mut input = Dummy::from_content("<t>ok</t>").with_index(index);
            state.apply(&mut input);
            assert_eq!(input.reasoning_content.unwrap(), "ok");
        }
    }

    #[test]
    fn should_buffer_partial_tags_between_chunks() {
        let mut chunks = [
            Dummy::from_content("me<thin"),
            Dummy::from_content("k>thoug"),
            Dummy::from_content("ht</thi"),
            Dummy::from_content("nk>ssage"),
        ];

        let mut state = ReasoningContentRemappingState::new((
            ReasoningContentPosition::content_unchecked("<think>", "</think>"),
            ReasoningContentPosition::ReasoningContent,
        ));

        for chunk in chunks.iter_mut() {
            state.apply(chunk);
        }

        assert_eq!(
            Dummy::from_chunks(chunks),
            Dummy::from_content("message").with_reasoning_content("thought")
        );
    }

    mod dummy {
        use crate::traits::reasoning_content_remapping::ReasoningContentRemappingInnerTarget;

        use super::*;

        use std::fmt::Display;
        #[derive(Debug, Default, PartialEq, Eq)]
        pub struct Dummy {
            pub index: usize,
            pub content: Option<String>,
            pub reasoning_content: Option<String>,
            pub reasoning: Option<String>,
        }

        impl Dummy {
            pub fn from_content(content: impl Display) -> Self {
                Self {
                    content: Some(content.to_string()),
                    ..Dummy::default()
                }
            }

            pub fn from_reasoning(reasoning: impl Display) -> Dummy {
                Dummy {
                    reasoning: Some(reasoning.to_string()),
                    ..Dummy::default()
                }
            }

            pub fn from_reasoning_content(reasoning_content: impl Display) -> Dummy {
                Dummy {
                    reasoning_content: Some(reasoning_content.to_string()),
                    ..Dummy::default()
                }
            }

            pub fn with_index(mut self, index: usize) -> Self {
                self.index = index;
                self
            }

            pub fn with_reasoning(mut self, reasoning: impl Display) -> Self {
                self.reasoning = Some(reasoning.to_string());
                self
            }

            pub fn with_reasoning_content(mut self, reasoning_content: impl Display) -> Self {
                self.reasoning_content = Some(reasoning_content.to_string());
                self
            }

            pub fn from_chunks(chunks: impl IntoIterator<Item = Self>) -> Self {
                chunks
                    .into_iter()
                    .fold(Dummy::default(), |prev, curr| prev + curr)
            }
        }

        impl std::ops::Add for Dummy {
            type Output = Dummy;

            fn add(mut self, rhs: Self) -> Self::Output {
                self.content = match (self.content, rhs.content) {
                    (Some(s), Some(r)) => Some(format!("{s}{r}")),
                    (Some(s), None) => Some(s),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                };
                self.reasoning_content = match (self.reasoning_content, rhs.reasoning_content) {
                    (Some(s), Some(r)) => Some(format!("{s}{r}")),
                    (Some(s), None) => Some(s),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                };
                self.reasoning = match (self.reasoning, rhs.reasoning) {
                    (Some(s), Some(r)) => Some(format!("{s}{r}")),
                    (Some(s), None) => Some(s),
                    (None, Some(r)) => Some(r),
                    (None, None) => None,
                };
                self
            }
        }

        impl ReasoningContentRemappingInnerTarget for Dummy {
            fn content_mut(&mut self) -> &mut Option<String> {
                &mut self.content
            }

            fn reasoning_content_mut(&mut self) -> &mut Option<String> {
                &mut self.reasoning_content
            }

            fn reasoning_mut(&mut self) -> &mut Option<String> {
                &mut self.reasoning
            }
        }

        impl ReasoningContentRemappingTarget for Dummy {
            type Inner = Dummy;

            fn index(&self) -> usize {
                self.index
            }

            fn inner_mut_opt(&mut self) -> Option<&mut Self::Inner> {
                Some(self)
            }
        }
    }
}
