use arrayvec::ArrayString;
use indexmap::IndexMap;

use crate::messages::models::api::{
    common::{ContentBlock, ContentBlockDelta},
    response::streaming::MessagesStreamEvent,
};

#[derive(..ApiModel, Copy, Hash)]
pub struct ThinkingRemappingConfig {
    pub from: ThinkingPosition,
    pub to: ThinkingPosition,
}

impl From<(ThinkingPosition, ThinkingPosition)> for ThinkingRemappingConfig {
    fn from((from, to): (ThinkingPosition, ThinkingPosition)) -> Self {
        Self { from, to }
    }
}

#[derive(..ApiModel, Copy, Hash)]
pub enum ThinkingPosition {
    ThinkingBlock,
    Text {
        start_tag: ArrayString<32>,
        stop_tag: ArrayString<32>,
    },
}

impl ThinkingPosition {
    pub fn text(
        start_tag: impl AsRef<str>,
        stop_tag: impl AsRef<str>,
    ) -> Result<Self, &'static str> {
        Ok(Self::Text {
            start_tag: ArrayString::from(start_tag.as_ref())
                .map_err(|_| "start_tag length must be lte 32")?,
            stop_tag: ArrayString::from(stop_tag.as_ref())
                .map_err(|_| "stop_tag length must be lte 32")?,
        })
    }

    pub fn text_unchecked(start_tag: impl AsRef<str>, stop_tag: impl AsRef<str>) -> Self {
        Self::text(start_tag, stop_tag).unwrap()
    }
}

pub struct ThinkingRemappingState {
    extractor: Extractor,
    emitter: Emitter,
}

impl ThinkingRemappingState {
    pub fn new(config: impl Into<ThinkingRemappingConfig>) -> Self {
        let ThinkingRemappingConfig { from, to } = config.into();

        let extractor = Extractor {
            tags: match from {
                ThinkingPosition::ThinkingBlock => None,
                ThinkingPosition::Text {
                    start_tag,
                    stop_tag,
                } => Some((start_tag, stop_tag)),
            },
            kinds: IndexMap::new(),
            scanners: IndexMap::new(),
        };

        let emitter = match to {
            ThinkingPosition::ThinkingBlock => Emitter::Blocks(BlockEmitter::default()),
            ThinkingPosition::Text {
                start_tag,
                stop_tag,
            } => Emitter::Tags(TagEmitter {
                start_tag,
                stop_tag,
                indexer: Indexer::default(),
                open: None,
                last_in: 0,
            }),
        };

        Self { extractor, emitter }
    }

    pub fn apply(&mut self, event: MessagesStreamEvent) -> Vec<MessagesStreamEvent> {
        let mut out = vec![];

        for item in self.extractor.apply(event) {
            self.emitter.emit(item, &mut out);
        }

        out
    }

    pub fn flush(&mut self) -> Vec<MessagesStreamEvent> {
        let mut out = vec![];
        self.emitter.flush(&mut out);
        out
    }
}

enum Item {
    TextStart(u32),
    Text(String),
    TextStop,
    ThinkingStart(u32),
    Thinking(String),
    ThinkingSignature(String),
    ThinkingStop,
    Block(u32, MessagesStreamEvent),
    Event(MessagesStreamEvent),
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Thinking,
    Other,
}

#[derive(Default)]
struct Scanner {
    inside: bool,
    held: String,
}

struct Extractor {
    tags: Option<(ArrayString<32>, ArrayString<32>)>,
    kinds: IndexMap<u32, Kind>,
    scanners: IndexMap<u32, Scanner>,
}

impl Extractor {
    fn apply(&mut self, event: MessagesStreamEvent) -> Vec<Item> {
        let mut items = vec![];

        let consumed = match &event {
            MessagesStreamEvent::ContentBlockStart {
                index,
                content_block,
                ..
            } => match content_block {
                ContentBlock::Text { text, .. } => {
                    self.kinds.insert(*index, Kind::Text);
                    if self.tags.is_some() {
                        self.scanners.insert(*index, Scanner::default());
                    }
                    items.push(Item::TextStart(*index));
                    self.feed_text(*index, text.clone(), &mut items);
                    true
                }
                ContentBlock::Thinking {
                    thinking,
                    signature,
                    ..
                } => {
                    self.kinds.insert(*index, Kind::Thinking);
                    items.push(Item::ThinkingStart(*index));
                    if !thinking.is_empty() {
                        items.push(Item::Thinking(thinking.clone()));
                    }
                    if !signature.is_empty() {
                        items.push(Item::ThinkingSignature(signature.clone()));
                    }
                    true
                }
                _ => {
                    self.kinds.insert(*index, Kind::Other);
                    false
                }
            },
            MessagesStreamEvent::ContentBlockDelta { index, delta, .. } => {
                match (self.kinds.get(index).copied(), delta) {
                    (Some(Kind::Text), ContentBlockDelta::TextDelta { text, .. }) => {
                        self.feed_text(*index, text.clone(), &mut items);
                        true
                    }
                    (Some(Kind::Thinking), ContentBlockDelta::ThinkingDelta { thinking, .. }) => {
                        items.push(Item::Thinking(thinking.clone()));
                        true
                    }
                    (Some(Kind::Thinking), ContentBlockDelta::SignatureDelta { signature, .. }) => {
                        items.push(Item::ThinkingSignature(signature.clone()));
                        true
                    }
                    _ => false,
                }
            }
            MessagesStreamEvent::ContentBlockStop { index, .. } => {
                match self.kinds.swap_remove(index) {
                    Some(Kind::Text) => {
                        self.finish_text(*index, &mut items);
                        true
                    }
                    Some(Kind::Thinking) => {
                        items.push(Item::ThinkingStop);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        if !consumed {
            items.push(match event.block_index() {
                Some(index) => Item::Block(index, event),
                None => Item::Event(event),
            });
        }

        items
    }

    fn feed_text(&mut self, index: u32, text: String, items: &mut Vec<Item>) {
        let Some((start_tag, stop_tag)) = self.tags else {
            if !text.is_empty() {
                items.push(Item::Text(text));
            }
            return;
        };

        let Some(scanner) = self.scanners.get_mut(&index) else {
            if !text.is_empty() {
                items.push(Item::Text(text));
            }
            return;
        };

        let mut buf = std::mem::take(&mut scanner.held);
        buf.push_str(&text);

        loop {
            let inside = scanner.inside;
            let tag = if inside { stop_tag } else { start_tag };

            if let Some(i) = buf.find(tag.as_str()) {
                let before = buf[..i].to_string();
                let rest = buf[i + tag.len()..].to_string();

                if inside {
                    if !before.is_empty() {
                        items.push(Item::Thinking(before));
                    }
                    items.push(Item::ThinkingStop);
                    items.push(Item::TextStart(index));
                } else {
                    if !before.is_empty() {
                        items.push(Item::Text(before));
                    }
                    items.push(Item::TextStop);
                    items.push(Item::ThinkingStart(index));
                }

                scanner.inside = !inside;
                buf = rest;
            } else {
                let keep = partial_suffix_len(&buf, tag.as_str());
                let split = buf.len() - keep;
                scanner.held = buf[split..].to_string();
                let emit = buf[..split].to_string();

                if !emit.is_empty() {
                    items.push(if inside {
                        Item::Thinking(emit)
                    } else {
                        Item::Text(emit)
                    });
                }

                break;
            }
        }
    }

    fn finish_text(&mut self, index: u32, items: &mut Vec<Item>) {
        match self.scanners.swap_remove(&index) {
            Some(Scanner { inside: true, held }) => {
                if !held.is_empty() {
                    items.push(Item::Thinking(held));
                }
                items.push(Item::ThinkingStop);
            }
            Some(Scanner {
                inside: false,
                held,
            }) => {
                if !held.is_empty() {
                    items.push(Item::Text(held));
                }
                items.push(Item::TextStop);
            }
            None => items.push(Item::TextStop),
        }
    }
}

fn partial_suffix_len(buf: &str, tag: &str) -> usize {
    (1..tag.len())
        .rev()
        .find(|&k| tag.is_char_boundary(k) && buf.ends_with(&tag[..k]))
        .unwrap_or(0)
}

#[derive(Default)]
struct Indexer {
    next: u32,
    map: IndexMap<u32, u32>,
}

impl Indexer {
    fn alloc(&mut self, in_index: u32) -> u32 {
        let out = self.next;
        self.next += 1;
        self.map.insert(in_index, out);
        out
    }

    fn rollback(&mut self, out: u32) {
        if self.next == out + 1 {
            self.next = out;
        }
        self.map.retain(|_, v| *v != out);
    }

    fn lookup(&self, in_index: u32) -> u32 {
        self.map.get(&in_index).copied().unwrap_or(in_index)
    }

    fn reindex(&mut self, in_index: u32, mut event: MessagesStreamEvent) -> MessagesStreamEvent {
        let out = match event {
            MessagesStreamEvent::ContentBlockStart { .. } => self.alloc(in_index),
            MessagesStreamEvent::ContentBlockStop { .. } => {
                let out = self.lookup(in_index);
                self.map.swap_remove(&in_index);
                out
            }
            _ => self.lookup(in_index),
        };

        event.set_block_index(out);
        event
    }
}

enum Emitter {
    Blocks(BlockEmitter),
    Tags(TagEmitter),
}

impl Emitter {
    fn emit(&mut self, item: Item, out: &mut Vec<MessagesStreamEvent>) {
        match self {
            Emitter::Blocks(e) => e.emit(item, out),
            Emitter::Tags(e) => e.emit(item, out),
        }
    }

    fn flush(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        match self {
            Emitter::Blocks(e) => e.flush(out),
            Emitter::Tags(e) => e.flush(out),
        }
    }
}

struct LazyText {
    out: u32,
    started: bool,
}

struct OpenThinking {
    out: u32,
    signature_seen: bool,
}

#[derive(Default)]
struct BlockEmitter {
    indexer: Indexer,
    text: Option<LazyText>,
    thinking: Option<OpenThinking>,
}

impl BlockEmitter {
    fn emit(&mut self, item: Item, out: &mut Vec<MessagesStreamEvent>) {
        match item {
            Item::TextStart(in_index) => {
                let out_index = self.indexer.alloc(in_index);
                self.text = Some(LazyText {
                    out: out_index,
                    started: false,
                });
            }
            Item::Text(text) => {
                if text.is_empty() {
                    return;
                }

                let lazy = self.text.get_or_insert_with(|| LazyText {
                    out: self.indexer.alloc(u32::MAX),
                    started: false,
                });

                if !lazy.started {
                    lazy.started = true;
                    out.push(MessagesStreamEvent::content_block_start(
                        lazy.out,
                        ContentBlock::text(""),
                    ));
                }

                out.push(MessagesStreamEvent::content_block_delta(
                    lazy.out,
                    ContentBlockDelta::text(text),
                ));
            }
            Item::TextStop => self.close_text(out),
            Item::ThinkingStart(in_index) => {
                let out_index = self.indexer.alloc(in_index);
                out.push(MessagesStreamEvent::content_block_start(
                    out_index,
                    ContentBlock::thinking("", ""),
                ));
                self.thinking = Some(OpenThinking {
                    out: out_index,
                    signature_seen: false,
                });
            }
            Item::Thinking(thinking) => {
                if let Some(open) = &self.thinking
                    && !thinking.is_empty()
                {
                    out.push(MessagesStreamEvent::content_block_delta(
                        open.out,
                        ContentBlockDelta::thinking(thinking),
                    ));
                }
            }
            Item::ThinkingSignature(signature) => {
                if let Some(open) = &mut self.thinking {
                    open.signature_seen = true;
                    out.push(MessagesStreamEvent::content_block_delta(
                        open.out,
                        ContentBlockDelta::signature(signature),
                    ));
                }
            }
            Item::ThinkingStop => self.close_thinking(out),
            Item::Block(in_index, event) => out.push(self.indexer.reindex(in_index, event)),
            Item::Event(event) => out.push(event),
        }
    }

    fn close_text(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        if let Some(lazy) = self.text.take() {
            if lazy.started {
                out.push(MessagesStreamEvent::content_block_stop(lazy.out));
            } else {
                self.indexer.rollback(lazy.out);
            }
        }
    }

    fn close_thinking(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        if let Some(open) = self.thinking.take() {
            if !open.signature_seen {
                out.push(MessagesStreamEvent::content_block_delta(
                    open.out,
                    ContentBlockDelta::signature(""),
                ));
            }
            out.push(MessagesStreamEvent::content_block_stop(open.out));
        }
    }

    fn flush(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        self.close_thinking(out);
        self.close_text(out);
    }
}

struct OpenText {
    out: u32,
    started: bool,
    pending_stop: bool,
}

struct TagEmitter {
    start_tag: ArrayString<32>,
    stop_tag: ArrayString<32>,
    indexer: Indexer,
    open: Option<OpenText>,
    last_in: u32,
}

impl TagEmitter {
    fn emit(&mut self, item: Item, out: &mut Vec<MessagesStreamEvent>) {
        match item {
            Item::TextStart(in_index) => {
                self.last_in = in_index;
                self.ensure_open(in_index).pending_stop = false;
            }
            Item::Text(text) => {
                if text.is_empty() {
                    return;
                }
                let in_index = self.last_in;
                self.ensure_open(in_index);
                self.push_text(text, out);
            }
            Item::TextStop => {
                if let Some(open) = &mut self.open {
                    if open.started {
                        open.pending_stop = true;
                    } else {
                        let open = self.open.take().unwrap();
                        self.indexer.rollback(open.out);
                    }
                }
            }
            Item::ThinkingStart(in_index) => {
                self.last_in = in_index;
                self.ensure_open(in_index).pending_stop = false;
                let start_tag = self.start_tag.to_string();
                self.push_text(start_tag, out);
            }
            Item::Thinking(thinking) => {
                if !thinking.is_empty() && self.open.is_some() {
                    self.push_text(thinking, out);
                }
            }
            Item::ThinkingSignature(_) => {}
            Item::ThinkingStop => {
                if self.open.is_some() {
                    let stop_tag = self.stop_tag.to_string();
                    self.push_text(stop_tag, out);
                    self.open.as_mut().unwrap().pending_stop = true;
                }
            }
            Item::Block(in_index, event) => {
                if matches!(event, MessagesStreamEvent::ContentBlockStart { .. }) {
                    self.flush_pending(out);
                }
                out.push(self.indexer.reindex(in_index, event));
            }
            Item::Event(event) => {
                if matches!(
                    event,
                    MessagesStreamEvent::MessageDelta { .. }
                        | MessagesStreamEvent::MessageStop { .. }
                ) {
                    self.flush_pending(out);
                }
                out.push(event);
            }
        }
    }

    fn ensure_open(&mut self, in_index: u32) -> &mut OpenText {
        match &self.open {
            Some(open) => {
                self.indexer.map.insert(in_index, open.out);
            }
            None => {
                let out_index = self.indexer.alloc(in_index);
                self.open = Some(OpenText {
                    out: out_index,
                    started: false,
                    pending_stop: false,
                });
            }
        }

        self.open.as_mut().unwrap()
    }

    fn push_text(&mut self, text: String, out: &mut Vec<MessagesStreamEvent>) {
        let Some(open) = &mut self.open else {
            return;
        };

        if !open.started {
            open.started = true;
            out.push(MessagesStreamEvent::content_block_start(
                open.out,
                ContentBlock::text(""),
            ));
        }

        out.push(MessagesStreamEvent::content_block_delta(
            open.out,
            ContentBlockDelta::text(text),
        ));
    }

    fn flush_pending(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        if self.open.as_ref().is_some_and(|o| o.pending_stop) {
            let open = self.open.take().unwrap();
            out.push(MessagesStreamEvent::content_block_stop(open.out));
        }
    }

    fn flush(&mut self, out: &mut Vec<MessagesStreamEvent>) {
        if let Some(open) = self.open.take() {
            if open.started {
                out.push(MessagesStreamEvent::content_block_stop(open.out));
            } else {
                self.indexer.rollback(open.out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::models::api::common::{ContentBlock, ContentBlockDelta};
    use serde_json::json;

    fn tags() -> ThinkingPosition {
        ThinkingPosition::text_unchecked("<think>", "</think>")
    }

    fn run(
        config: (ThinkingPosition, ThinkingPosition),
        input: Vec<MessagesStreamEvent>,
    ) -> Vec<MessagesStreamEvent> {
        let mut state = ThinkingRemappingState::new(config);
        let mut out = vec![];
        for event in input {
            out.extend(state.apply(event));
        }
        out.extend(state.flush());
        out
    }

    fn start(index: u32, block: ContentBlock) -> MessagesStreamEvent {
        MessagesStreamEvent::content_block_start(index, block)
    }
    fn text_start(index: u32) -> MessagesStreamEvent {
        start(index, ContentBlock::text(""))
    }
    fn thinking_start(index: u32) -> MessagesStreamEvent {
        start(index, ContentBlock::thinking("", ""))
    }
    fn text(index: u32, s: &str) -> MessagesStreamEvent {
        MessagesStreamEvent::content_block_delta(index, ContentBlockDelta::text(s))
    }
    fn thinking(index: u32, s: &str) -> MessagesStreamEvent {
        MessagesStreamEvent::content_block_delta(index, ContentBlockDelta::thinking(s))
    }
    fn signature(index: u32, s: &str) -> MessagesStreamEvent {
        MessagesStreamEvent::content_block_delta(index, ContentBlockDelta::signature(s))
    }
    fn stop(index: u32) -> MessagesStreamEvent {
        MessagesStreamEvent::content_block_stop(index)
    }
    fn tool_use_start(index: u32) -> MessagesStreamEvent {
        start(
            index,
            serde_json::from_value(
                json!({"type": "tool_use", "id": "t1", "name": "f", "input": {}}),
            )
            .unwrap(),
        )
    }
    fn input_json(index: u32, s: &str) -> MessagesStreamEvent {
        serde_json::from_value(json!({"type": "content_block_delta", "index": index, "delta": {"type": "input_json_delta", "partial_json": s}})).unwrap()
    }
    fn message_delta() -> MessagesStreamEvent {
        serde_json::from_value(json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 3}})).unwrap()
    }
    fn message_stop() -> MessagesStreamEvent {
        serde_json::from_value(json!({"type": "message_stop"})).unwrap()
    }
    fn ping() -> MessagesStreamEvent {
        serde_json::from_value(json!({"type": "ping"})).unwrap()
    }

    #[test]
    fn thinking_block_becomes_tags_merged_into_the_following_text_block() {
        let out = run(
            (ThinkingPosition::ThinkingBlock, tags()),
            vec![
                thinking_start(0),
                thinking(0, "a"),
                thinking(0, "b"),
                signature(0, "sig"),
                stop(0),
                text_start(1),
                text(1, "c"),
                stop(1),
                message_delta(),
                message_stop(),
            ],
        );

        assert_eq!(
            out,
            vec![
                text_start(0),
                text(0, "<think>"),
                text(0, "a"),
                text(0, "b"),
                text(0, "</think>"),
                text(0, "c"),
                stop(0),
                message_delta(),
                message_stop(),
            ]
        );
    }

    #[test]
    fn thinking_block_to_tags_closes_text_block_before_a_tool_use_block() {
        let out = run(
            (ThinkingPosition::ThinkingBlock, tags()),
            vec![
                thinking_start(0),
                thinking(0, "a"),
                stop(0),
                tool_use_start(1),
                input_json(1, "{}"),
                stop(1),
                message_stop(),
            ],
        );

        assert_eq!(
            out,
            vec![
                text_start(0),
                text(0, "<think>"),
                text(0, "a"),
                text(0, "</think>"),
                stop(0),
                tool_use_start(1),
                input_json(1, "{}"),
                stop(1),
                message_stop(),
            ]
        );
    }

    #[test]
    fn thinking_block_to_tags_defers_the_stop_until_flush() {
        let mut state = ThinkingRemappingState::new((ThinkingPosition::ThinkingBlock, tags()));
        let mut out = vec![];
        for event in [thinking_start(0), thinking(0, "a"), stop(0)] {
            out.extend(state.apply(event));
        }
        assert_eq!(
            out,
            vec![
                text_start(0),
                text(0, "<think>"),
                text(0, "a"),
                text(0, "</think>")
            ]
        );
        assert_eq!(state.flush(), vec![stop(0)]);
    }

    #[test]
    fn tags_become_a_thinking_block_across_split_deltas() {
        let out = run(
            (tags(), ThinkingPosition::ThinkingBlock),
            vec![
                text_start(0),
                text(0, "<thi"),
                text(0, "nk>pl"),
                text(0, "an</think>ans"),
                text(0, "wer"),
                stop(0),
                message_stop(),
            ],
        );

        assert_eq!(
            out,
            vec![
                thinking_start(0),
                thinking(0, "pl"),
                thinking(0, "an"),
                signature(0, ""),
                stop(0),
                text_start(1),
                text(1, "ans"),
                text(1, "wer"),
                stop(1),
                message_stop(),
            ]
        );
    }

    #[test]
    fn tags_in_the_middle_of_text_split_the_text_block_in_three() {
        let out = run(
            (tags(), ThinkingPosition::ThinkingBlock),
            vec![
                text_start(0),
                text(0, "hello <think>x</think> bye"),
                stop(0),
            ],
        );

        assert_eq!(
            out,
            vec![
                text_start(0),
                text(0, "hello "),
                stop(0),
                thinking_start(1),
                thinking(1, "x"),
                signature(1, ""),
                stop(1),
                text_start(2),
                text(2, " bye"),
                stop(2),
            ]
        );
    }

    #[test]
    fn text_without_tags_is_unchanged() {
        let input = vec![
            text_start(0),
            text(0, "plain"),
            text(0, " text"),
            stop(0),
            message_delta(),
        ];
        let out = run((tags(), ThinkingPosition::ThinkingBlock), input.clone());
        assert_eq!(out, input);
    }

    #[test]
    fn a_false_partial_tag_is_released_as_text() {
        let out = run(
            (tags(), ThinkingPosition::ThinkingBlock),
            vec![text_start(0), text(0, "a <th"), text(0, "ing"), stop(0)],
        );
        assert_eq!(
            out,
            vec![text_start(0), text(0, "a "), text(0, "<thing"), stop(0)]
        );
    }

    #[test]
    fn native_thinking_block_keeps_its_signature_when_target_is_thinking_block() {
        let input = vec![
            thinking_start(0),
            thinking(0, "a"),
            signature(0, "real"),
            stop(0),
            text_start(1),
            text(1, "b"),
            stop(1),
        ];
        let out = run((tags(), ThinkingPosition::ThinkingBlock), input.clone());
        assert_eq!(out, input);
    }

    #[test]
    fn tag_to_tag_renames_inside_one_text_block() {
        let out = run(
            (tags(), ThinkingPosition::text_unchecked("[T]", "[/T]")),
            vec![
                text_start(0),
                text(0, "<think>a</think>b"),
                stop(0),
                message_stop(),
            ],
        );
        assert_eq!(
            out,
            vec![
                text_start(0),
                text(0, "[T]"),
                text(0, "a"),
                text(0, "[/T]"),
                text(0, "b"),
                stop(0),
                message_stop()
            ]
        );
    }

    #[test]
    fn non_block_events_pass_through_and_other_blocks_are_reindexed() {
        let redacted: ContentBlock =
            serde_json::from_value(json!({"type": "redacted_thinking", "data": "xyz"})).unwrap();
        let out = run(
            (ThinkingPosition::ThinkingBlock, tags()),
            vec![
                ping(),
                start(0, redacted.clone()),
                stop(0),
                thinking_start(1),
                thinking(1, "a"),
                stop(1),
                text_start(2),
                text(2, "b"),
                stop(2),
                ping(),
                message_stop(),
            ],
        );

        assert_eq!(
            out,
            vec![
                ping(),
                start(0, redacted),
                stop(0),
                text_start(1),
                text(1, "<think>"),
                text(1, "a"),
                text(1, "</think>"),
                text(1, "b"),
                ping(),
                stop(1),
                message_stop(),
            ]
        );
    }
}
