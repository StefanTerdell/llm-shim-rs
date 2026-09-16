use arrayvec::ArrayString;
use indexmap::IndexMap;
use std::collections::HashSet;

use crate::responses::models::{
    api::{
        common::{ContentPart, OutputItem, ReasoningPart},
        response::streaming::ResponsesStreamEvent,
    },
    lib::assembler::ResponseAssembler,
};

#[derive(..ApiModel, Copy, Eq, Hash)]
pub struct ResponsesReasoningRemappingConfig {
    pub from: ResponsesReasoningPosition,
    pub to: ResponsesReasoningPosition,
}

impl From<(ResponsesReasoningPosition, ResponsesReasoningPosition)>
    for ResponsesReasoningRemappingConfig
{
    fn from((from, to): (ResponsesReasoningPosition, ResponsesReasoningPosition)) -> Self {
        Self { from, to }
    }
}

#[derive(..ApiModel, Copy, Eq, Hash)]
pub enum ResponsesReasoningPosition {
    Summary,
    Content,
    Text {
        start_tag: ArrayString<32>,
        stop_tag: ArrayString<32>,
    },
}

impl ResponsesReasoningPosition {
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

pub struct ResponsesReasoningRemappingState {
    extractor: Extractor,
    emitter: Emitter,
}

impl ResponsesReasoningRemappingState {
    pub fn new(config: impl Into<ResponsesReasoningRemappingConfig>) -> Self {
        let ResponsesReasoningRemappingConfig { from, to } = config.into();

        Self {
            extractor: Extractor {
                tags: match from {
                    ResponsesReasoningPosition::Text {
                        start_tag,
                        stop_tag,
                    } => Some((start_tag, stop_tag)),
                    _ => None,
                },
                kinds: IndexMap::new(),
                message: None,
                reasoning: None,
            },
            emitter: Emitter {
                to,
                assembler: ResponseAssembler::default(),
                next_out: 0,
                item_map: IndexMap::new(),
                seq: None,
                message: None,
                reasoning: None,
                used_ids: HashSet::new(),
                synth_messages: 0,
                synth_reasonings: 0,
                tag_parts: 0,
            },
        }
    }

    pub fn apply(&mut self, event: ResponsesStreamEvent) -> Vec<ResponsesStreamEvent> {
        if self.emitter.seq.is_none() {
            self.emitter.seq = event.sequence_number();
        }

        let mut out = vec![];

        for item in self.extractor.apply(event) {
            self.emitter.emit_item(item, &mut out);
        }

        out
    }

    pub fn flush(&mut self) -> Vec<ResponsesStreamEvent> {
        let mut out = vec![];
        self.emitter.flush_all(&mut out);
        out
    }
}

enum Item {
    MessageStart { id: Option<String> },
    TextStart,
    Text(String),
    TextStop,
    MessageStop,
    ReasoningStart { id: Option<String> },
    ReasoningPartStart,
    Reasoning(String),
    ReasoningPartStop,
    ReasoningStop,
    MessagePart(ResponsesStreamEvent),
    Passthrough(ResponsesStreamEvent),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Message,
    Reasoning,
    Other,
}

#[derive(Default)]
struct Scanner {
    inside: bool,
    held: String,
}

struct MessageState {
    index: u32,
    id: Option<String>,
    saw_text: bool,
    part_open: bool,
    stopped: bool,
    scanner: Scanner,
}

struct ReasoningState {
    index: u32,
    saw_text: bool,
    part_open: bool,
}

struct Extractor {
    tags: Option<(ArrayString<32>, ArrayString<32>)>,
    kinds: IndexMap<u32, Kind>,
    message: Option<MessageState>,
    reasoning: Option<ReasoningState>,
}

impl Extractor {
    fn apply(&mut self, event: ResponsesStreamEvent) -> Vec<Item> {
        let mut items = vec![];

        let consumed = match &event {
            ResponsesStreamEvent::OutputItemAdded {
                output_index, item, ..
            } => match item {
                OutputItem::Message { id, content, .. } => {
                    self.start_message(*output_index, id.clone(), &mut items);
                    self.message_parts(content, &mut items);
                    true
                }
                OutputItem::Reasoning {
                    id,
                    summary,
                    content,
                    ..
                } => {
                    self.start_reasoning(*output_index, id.clone(), &mut items);
                    self.reasoning_parts(summary, content.as_deref(), &mut items);
                    true
                }
                _ => {
                    self.kinds.insert(*output_index, Kind::Other);
                    false
                }
            },
            ResponsesStreamEvent::OutputItemDone {
                output_index, item, ..
            } => match (self.kinds.get(output_index).copied(), item) {
                (Some(Kind::Message) | None, OutputItem::Message { id, content, .. }) => {
                    if self.current_message(*output_index).is_none() {
                        self.start_message(*output_index, id.clone(), &mut items);
                    }
                    if !self.message.as_ref().is_some_and(|m| m.saw_text) {
                        self.message_parts(content, &mut items);
                    }
                    self.finish_message(&mut items);
                    true
                }
                (
                    Some(Kind::Reasoning) | None,
                    OutputItem::Reasoning {
                        id,
                        summary,
                        content,
                        ..
                    },
                ) => {
                    if self.current_reasoning(*output_index).is_none() {
                        self.start_reasoning(*output_index, id.clone(), &mut items);
                    }
                    if !self.reasoning.as_ref().is_some_and(|r| r.saw_text) {
                        self.reasoning_parts(summary, content.as_deref(), &mut items);
                    }
                    self.finish_reasoning(&mut items);
                    true
                }
                _ => {
                    self.kinds.swap_remove(output_index);
                    false
                }
            },
            ResponsesStreamEvent::ContentPartAdded {
                output_index,
                item_id,
                part,
                ..
            } => match (self.kinds.get(output_index).copied(), part) {
                (Some(Kind::Message) | None, ContentPart::OutputText { text, .. }) => {
                    self.ensure_message(*output_index, item_id.clone(), &mut items);
                    self.start_text(&mut items);
                    let text = text.clone();
                    self.feed(text, &mut items);
                    true
                }
                (Some(Kind::Reasoning) | None, ContentPart::ReasoningText { text, .. }) => {
                    self.ensure_reasoning(*output_index, item_id.clone(), &mut items);
                    self.start_reasoning_part(&mut items);
                    if !text.is_empty() {
                        items.push(Item::Reasoning(text.clone()));
                    }
                    true
                }
                (Some(Kind::Message), _) => {
                    items.push(Item::MessagePart(event));
                    return items;
                }
                _ => false,
            },
            ResponsesStreamEvent::ContentPartDone {
                output_index, part, ..
            } => match (self.kinds.get(output_index).copied(), part) {
                (Some(Kind::Message), ContentPart::OutputText { .. }) => {
                    self.stop_text(&mut items);
                    true
                }
                (Some(Kind::Reasoning), ContentPart::ReasoningText { .. }) => {
                    self.stop_reasoning_part(&mut items);
                    true
                }
                (Some(Kind::Message), _) => {
                    items.push(Item::MessagePart(event));
                    return items;
                }
                _ => false,
            },
            ResponsesStreamEvent::OutputTextDelta {
                output_index,
                item_id,
                delta,
                ..
            } => {
                self.ensure_message(*output_index, item_id.clone(), &mut items);
                if !self.message.as_ref().is_some_and(|m| m.part_open) {
                    self.start_text(&mut items);
                }
                if let Some(m) = &mut self.message {
                    m.saw_text = true;
                }
                let delta = delta.clone();
                self.feed(delta, &mut items);
                true
            }
            ResponsesStreamEvent::OutputTextDone { output_index, .. } => {
                self.kinds.get(output_index) == Some(&Kind::Message)
            }
            ResponsesStreamEvent::RefusalDelta { output_index, .. }
            | ResponsesStreamEvent::RefusalDone { output_index, .. } => {
                if self.kinds.get(output_index) == Some(&Kind::Message) {
                    items.push(Item::MessagePart(event));
                    return items;
                }
                false
            }
            ResponsesStreamEvent::ReasoningSummaryPartAdded {
                output_index,
                item_id,
                part,
                ..
            } => {
                self.ensure_reasoning(*output_index, item_id.clone(), &mut items);
                self.start_reasoning_part(&mut items);
                if let Some(text) = part.text()
                    && !text.is_empty()
                {
                    items.push(Item::Reasoning(text.to_string()));
                }
                true
            }
            ResponsesStreamEvent::ReasoningSummaryTextDelta {
                output_index,
                item_id,
                delta,
                ..
            }
            | ResponsesStreamEvent::ReasoningTextDelta {
                output_index,
                item_id,
                delta,
                ..
            } => {
                self.ensure_reasoning(*output_index, item_id.clone(), &mut items);
                if !self.reasoning.as_ref().is_some_and(|r| r.part_open) {
                    self.start_reasoning_part(&mut items);
                }
                if let Some(r) = &mut self.reasoning {
                    r.saw_text = true;
                }
                if !delta.is_empty() {
                    items.push(Item::Reasoning(delta.clone()));
                }
                true
            }
            ResponsesStreamEvent::ReasoningSummaryTextDone { output_index, .. }
            | ResponsesStreamEvent::ReasoningTextDone { output_index, .. } => {
                self.kinds.get(output_index) == Some(&Kind::Reasoning)
            }
            ResponsesStreamEvent::ReasoningSummaryPartDone { output_index, .. }
                if self.kinds.get(output_index) == Some(&Kind::Reasoning) =>
            {
                self.stop_reasoning_part(&mut items);
                true
            }
            _ => false,
        };

        if !consumed {
            items.push(Item::Passthrough(event));
        }

        items
    }

    fn current_message(&self, index: u32) -> Option<&MessageState> {
        self.message.as_ref().filter(|m| m.index == index)
    }

    fn current_reasoning(&self, index: u32) -> Option<&ReasoningState> {
        self.reasoning.as_ref().filter(|r| r.index == index)
    }

    fn ensure_message(&mut self, index: u32, id: Option<String>, items: &mut Vec<Item>) {
        if self.current_message(index).is_none() {
            self.start_message(index, id, items);
        }
    }

    fn ensure_reasoning(&mut self, index: u32, id: Option<String>, items: &mut Vec<Item>) {
        if self.current_reasoning(index).is_none() {
            self.start_reasoning(index, id, items);
        }
    }

    fn start_message(&mut self, index: u32, id: Option<String>, items: &mut Vec<Item>) {
        self.finish_message(items);
        self.finish_reasoning(items);
        self.kinds.insert(index, Kind::Message);
        items.push(Item::MessageStart { id: id.clone() });
        self.message = Some(MessageState {
            index,
            id,
            saw_text: false,
            part_open: false,
            stopped: false,
            scanner: Scanner::default(),
        });
    }

    fn finish_message(&mut self, items: &mut Vec<Item>) {
        if self.message.is_none() {
            return;
        }

        self.stop_text(items);

        let m = self.message.take().unwrap();

        if !m.stopped {
            items.push(Item::MessageStop);
        }

        self.kinds.swap_remove(&m.index);
    }

    fn message_parts(&mut self, content: &[ContentPart], items: &mut Vec<Item>) {
        for part in content {
            if let ContentPart::OutputText { text, .. } = part {
                self.start_text(items);
                if !text.is_empty()
                    && let Some(m) = &mut self.message
                {
                    m.saw_text = true;
                }
                self.feed(text.clone(), items);
                self.stop_text(items);
            }
        }
    }

    fn start_text(&mut self, items: &mut Vec<Item>) {
        let Some(m) = &mut self.message else {
            return;
        };

        if m.part_open {
            self.stop_text(items);
        }

        let m = self.message.as_mut().unwrap();
        m.part_open = true;

        if m.stopped && !m.scanner.inside {
            items.push(Item::MessageStart { id: m.id.clone() });
            m.stopped = false;
        }

        if !m.scanner.inside {
            items.push(Item::TextStart);
        }
    }

    fn stop_text(&mut self, items: &mut Vec<Item>) {
        let Some(m) = &mut self.message else {
            return;
        };

        if !m.part_open {
            return;
        }

        m.part_open = false;
        let held = std::mem::take(&mut m.scanner.held);

        if m.scanner.inside {
            if !held.is_empty() {
                items.push(Item::Reasoning(held));
            }
            items.push(Item::ReasoningPartStop);
            items.push(Item::ReasoningStop);
            m.scanner.inside = false;
        } else {
            if !held.is_empty() {
                items.push(Item::Text(held));
            }
            items.push(Item::TextStop);
        }
    }

    fn feed(&mut self, text: String, items: &mut Vec<Item>) {
        let Some((start_tag, stop_tag)) = self.tags else {
            if !text.is_empty() {
                items.push(Item::Text(text));
            }
            return;
        };

        let Some(m) = &mut self.message else {
            return;
        };

        let mut buf = std::mem::take(&mut m.scanner.held);
        buf.push_str(&text);

        loop {
            let inside = m.scanner.inside;
            let tag = if inside { stop_tag } else { start_tag };

            if let Some(i) = buf.find(tag.as_str()) {
                let before = buf[..i].to_string();
                let rest = buf[i + tag.len()..].to_string();

                if inside {
                    if !before.is_empty() {
                        items.push(Item::Reasoning(before));
                    }
                    items.push(Item::ReasoningPartStop);
                    items.push(Item::ReasoningStop);
                    items.push(Item::MessageStart { id: m.id.clone() });
                    m.stopped = false;
                    items.push(Item::TextStart);
                } else {
                    if !before.is_empty() {
                        items.push(Item::Text(before));
                    }
                    items.push(Item::TextStop);
                    items.push(Item::MessageStop);
                    m.stopped = true;
                    items.push(Item::ReasoningStart { id: None });
                    items.push(Item::ReasoningPartStart);
                }

                m.scanner.inside = !inside;
                buf = rest;
            } else {
                let keep = partial_suffix_len(&buf, tag.as_str());
                let split = buf.len() - keep;
                m.scanner.held = buf[split..].to_string();
                let emit = buf[..split].to_string();

                if !emit.is_empty() {
                    items.push(if inside {
                        Item::Reasoning(emit)
                    } else {
                        Item::Text(emit)
                    });
                }

                break;
            }
        }
    }

    fn start_reasoning(&mut self, index: u32, id: Option<String>, items: &mut Vec<Item>) {
        self.finish_reasoning(items);
        self.finish_message(items);
        self.kinds.insert(index, Kind::Reasoning);
        items.push(Item::ReasoningStart { id });
        self.reasoning = Some(ReasoningState {
            index,
            saw_text: false,
            part_open: false,
        });
    }

    fn finish_reasoning(&mut self, items: &mut Vec<Item>) {
        if self.reasoning.is_none() {
            return;
        }

        self.stop_reasoning_part(items);
        let r = self.reasoning.take().unwrap();
        items.push(Item::ReasoningStop);
        self.kinds.swap_remove(&r.index);
    }

    fn reasoning_parts(
        &mut self,
        summary: &[ReasoningPart],
        content: Option<&[ReasoningPart]>,
        items: &mut Vec<Item>,
    ) {
        for part in summary.iter().chain(content.into_iter().flatten()) {
            if let Some(text) = part.text() {
                self.start_reasoning_part(items);
                if !text.is_empty() {
                    items.push(Item::Reasoning(text.to_string()));
                    if let Some(r) = &mut self.reasoning {
                        r.saw_text = true;
                    }
                }
                self.stop_reasoning_part(items);
            }
        }
    }

    fn start_reasoning_part(&mut self, items: &mut Vec<Item>) {
        if self.reasoning.as_ref().is_some_and(|r| r.part_open) {
            self.stop_reasoning_part(items);
        }

        if let Some(r) = &mut self.reasoning {
            r.part_open = true;
            items.push(Item::ReasoningPartStart);
        }
    }

    fn stop_reasoning_part(&mut self, items: &mut Vec<Item>) {
        if let Some(r) = &mut self.reasoning
            && r.part_open
        {
            r.part_open = false;
            items.push(Item::ReasoningPartStop);
        }
    }
}

fn partial_suffix_len(buf: &str, tag: &str) -> usize {
    (1..tag.len())
        .rev()
        .find(|&k| tag.is_char_boundary(k) && buf.ends_with(&tag[..k]))
        .unwrap_or(0)
}

struct OpenMessage {
    out: Option<u32>,
    id: String,
    requested_id: Option<String>,
    part_ci: Option<u32>,
    next_ci: u32,
    part_map: IndexMap<u32, u32>,
    pending_close: bool,
}

impl OpenMessage {
    fn new(requested_id: Option<String>) -> Self {
        Self {
            out: None,
            id: String::new(),
            requested_id,
            part_ci: None,
            next_ci: 0,
            part_map: IndexMap::new(),
            pending_close: false,
        }
    }
}

struct OpenReasoning {
    out: u32,
    id: String,
    part: Option<u32>,
    next_part: u32,
}

struct Emitter {
    to: ResponsesReasoningPosition,
    assembler: ResponseAssembler,
    next_out: u32,
    item_map: IndexMap<u32, u32>,
    seq: Option<u64>,
    message: Option<OpenMessage>,
    reasoning: Option<OpenReasoning>,
    used_ids: HashSet<String>,
    synth_messages: u32,
    synth_reasonings: u32,
    tag_parts: u32,
}

impl Emitter {
    fn emit_item(&mut self, item: Item, out: &mut Vec<ResponsesStreamEvent>) {
        match self.to {
            ResponsesReasoningPosition::Text {
                start_tag,
                stop_tag,
            } => match item {
                Item::MessageStart { id } => match &mut self.message {
                    Some(m) if m.pending_close => {
                        m.pending_close = false;
                        if m.out.is_none() {
                            m.requested_id = id;
                        }
                    }
                    _ => {
                        self.close_message(out);
                        self.message = Some(OpenMessage::new(id));
                    }
                },
                Item::TextStart => {
                    if self.message.is_none() {
                        self.message = Some(OpenMessage::new(None));
                    }
                }
                Item::Text(text) => self.append_text(text, out),
                Item::TextStop => self.close_text_part(out),
                Item::MessageStop => self.close_message(out),
                Item::ReasoningStart { .. } => {
                    match &mut self.message {
                        Some(m) => m.pending_close = false,
                        None => self.message = Some(OpenMessage::new(None)),
                    }
                    self.tag_parts = 0;
                    self.append_text(start_tag.to_string(), out);
                }
                Item::ReasoningPartStart => {
                    if self.tag_parts > 0 {
                        self.append_text("\n\n".to_string(), out);
                    }
                    self.tag_parts += 1;
                }
                Item::Reasoning(text) => self.append_text(text, out),
                Item::ReasoningPartStop => {}
                Item::ReasoningStop => {
                    self.append_text(stop_tag.to_string(), out);
                    if let Some(m) = &mut self.message {
                        m.pending_close = true;
                    }
                }
                Item::MessagePart(event) => self.message_part(event, out),
                Item::Passthrough(event) => self.passthrough(event, out),
            },
            ResponsesReasoningPosition::Summary | ResponsesReasoningPosition::Content => match item
            {
                Item::MessageStart { id } => {
                    self.close_message(out);
                    self.message = Some(OpenMessage::new(id));
                }
                Item::TextStart => {
                    if self.message.is_none() {
                        self.message = Some(OpenMessage::new(None));
                    }
                }
                Item::Text(text) => self.append_text(text, out),
                Item::TextStop => self.close_text_part(out),
                Item::MessageStop => self.close_message(out),
                Item::ReasoningStart { id } => self.open_reasoning(id, out),
                Item::ReasoningPartStart => self.open_reasoning_part(out),
                Item::Reasoning(text) => self.append_reasoning(text, out),
                Item::ReasoningPartStop => self.close_reasoning_part(out),
                Item::ReasoningStop => self.close_reasoning(out),
                Item::MessagePart(event) => self.message_part(event, out),
                Item::Passthrough(event) => self.passthrough(event, out),
            },
        }
    }

    fn emit(&mut self, mut event: ResponsesStreamEvent, out: &mut Vec<ResponsesStreamEvent>) {
        if let Some(seq) = &mut self.seq {
            event.set_sequence_number(*seq);
            *seq += 1;
        }

        self.assembler.apply(&event);
        out.push(event);
    }

    fn synth_id(&mut self, requested: Option<String>, prefix: &str) -> String {
        let id = match requested {
            Some(id) if !self.used_ids.contains(&id) => id,
            _ => {
                let counter = if prefix == "msg" {
                    &mut self.synth_messages
                } else {
                    &mut self.synth_reasonings
                };
                let id = format!("{prefix}_remap_{counter}");
                *counter += 1;
                id
            }
        };

        self.used_ids.insert(id.clone());
        id
    }

    fn start_message_now(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        if self.message.is_none() {
            self.message = Some(OpenMessage::new(None));
        }

        if self.message.as_ref().unwrap().out.is_some() {
            return;
        }

        let requested = self.message.as_mut().unwrap().requested_id.take();
        let id = self.synth_id(requested, "msg");
        let output_index = self.next_out;
        self.next_out += 1;

        let m = self.message.as_mut().unwrap();
        m.out = Some(output_index);
        m.id = id.clone();

        let mut item = OutputItem::message(Some(id));
        if let Some(status) = item.status_mut() {
            *status = Some("in_progress".to_string());
        }

        self.emit(
            ResponsesStreamEvent::OutputItemAdded {
                output_index,
                item,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn start_text_part_now(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        self.start_message_now(out);

        let m = self.message.as_mut().unwrap();

        if m.part_ci.is_some() {
            return;
        }

        let content_index = m.next_ci;
        m.next_ci += 1;
        m.part_ci = Some(content_index);
        let item_id = Some(m.id.clone());
        let output_index = m.out.unwrap();

        self.emit(
            ResponsesStreamEvent::ContentPartAdded {
                item_id,
                output_index,
                content_index,
                part: ContentPart::output_text(""),
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn append_text(&mut self, text: String, out: &mut Vec<ResponsesStreamEvent>) {
        if text.is_empty() {
            return;
        }

        self.start_text_part_now(out);

        let m = self.message.as_ref().unwrap();
        let event = ResponsesStreamEvent::OutputTextDelta {
            item_id: Some(m.id.clone()),
            output_index: m.out.unwrap(),
            content_index: m.part_ci.unwrap(),
            delta: text,
            logprobs: Some(vec![]),
            sequence_number: None,
            additional_properties: Default::default(),
        };

        self.emit(event, out);
    }

    fn close_text_part(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        let Some(m) = &mut self.message else {
            return;
        };

        let (Some(output_index), Some(content_index)) = (m.out, m.part_ci.take()) else {
            return;
        };

        let item_id = Some(m.id.clone());

        let part = match self.assembler.item(output_index) {
            Some(OutputItem::Message { content, .. }) => {
                content.get(content_index as usize).cloned()
            }
            _ => None,
        }
        .unwrap_or_else(|| ContentPart::output_text(""));

        let text = match &part {
            ContentPart::OutputText { text, .. } => text.clone(),
            _ => String::new(),
        };

        self.emit(
            ResponsesStreamEvent::OutputTextDone {
                item_id: item_id.clone(),
                output_index,
                content_index,
                text,
                logprobs: Some(vec![]),
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );

        self.emit(
            ResponsesStreamEvent::ContentPartDone {
                item_id,
                output_index,
                content_index,
                part,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn close_message(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        if self.message.is_none() {
            return;
        }

        self.close_text_part(out);

        let m = self.message.take().unwrap();

        let Some(output_index) = m.out else {
            return;
        };

        if let Some(item) = self.assembler.item_mut(output_index)
            && let Some(status) = item.status_mut()
        {
            *status = Some("completed".to_string());
        }

        let item = self
            .assembler
            .item(output_index)
            .cloned()
            .unwrap_or_else(|| OutputItem::message(Some(m.id.clone())));

        self.emit(
            ResponsesStreamEvent::OutputItemDone {
                output_index,
                item,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn message_part(
        &mut self,
        mut event: ResponsesStreamEvent,
        out: &mut Vec<ResponsesStreamEvent>,
    ) {
        self.start_message_now(out);

        let m = self.message.as_mut().unwrap();

        if let Some(in_ci) = event.content_index() {
            let out_ci = match m.part_map.get(&in_ci) {
                Some(ci) => *ci,
                None => {
                    let ci = m.next_ci;
                    m.next_ci += 1;
                    m.part_map.insert(in_ci, ci);
                    ci
                }
            };
            event.set_content_index(out_ci);
        }

        event.set_output_index(m.out.unwrap());
        event.set_item_id(&m.id.clone());
        self.emit(event, out);
    }

    fn open_reasoning(&mut self, id: Option<String>, out: &mut Vec<ResponsesStreamEvent>) {
        self.close_reasoning(out);
        self.close_message(out);

        let id = self.synth_id(id, "rs");
        let output_index = self.next_out;
        self.next_out += 1;

        let mut item = OutputItem::reasoning(Some(id.clone()));
        if self.to == ResponsesReasoningPosition::Content
            && let OutputItem::Reasoning { content, .. } = &mut item
        {
            *content = Some(vec![]);
        }

        self.reasoning = Some(OpenReasoning {
            out: output_index,
            id,
            part: None,
            next_part: 0,
        });

        self.emit(
            ResponsesStreamEvent::OutputItemAdded {
                output_index,
                item,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn open_reasoning_part(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        if self.reasoning.is_none() {
            self.open_reasoning(None, out);
        }

        if self.reasoning.as_ref().unwrap().part.is_some() {
            self.close_reasoning_part(out);
        }

        let r = self.reasoning.as_mut().unwrap();
        let index = r.next_part;
        r.next_part += 1;
        r.part = Some(index);
        let item_id = Some(r.id.clone());
        let output_index = r.out;

        let event = match self.to {
            ResponsesReasoningPosition::Content => ResponsesStreamEvent::ContentPartAdded {
                item_id,
                output_index,
                content_index: index,
                part: ContentPart::reasoning_text(""),
                sequence_number: None,
                additional_properties: Default::default(),
            },
            _ => ResponsesStreamEvent::ReasoningSummaryPartAdded {
                item_id,
                output_index,
                summary_index: index,
                part: ReasoningPart::summary_text(""),
                sequence_number: None,
                additional_properties: Default::default(),
            },
        };

        self.emit(event, out);
    }

    fn append_reasoning(&mut self, text: String, out: &mut Vec<ResponsesStreamEvent>) {
        if text.is_empty() {
            return;
        }

        if self.reasoning.as_ref().is_none_or(|r| r.part.is_none()) {
            self.open_reasoning_part(out);
        }

        let r = self.reasoning.as_ref().unwrap();
        let item_id = Some(r.id.clone());
        let output_index = r.out;
        let index = r.part.unwrap();

        let event = match self.to {
            ResponsesReasoningPosition::Content => ResponsesStreamEvent::ReasoningTextDelta {
                item_id,
                output_index,
                content_index: index,
                delta: text,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            _ => ResponsesStreamEvent::ReasoningSummaryTextDelta {
                item_id,
                output_index,
                summary_index: index,
                delta: text,
                sequence_number: None,
                additional_properties: Default::default(),
            },
        };

        self.emit(event, out);
    }

    fn close_reasoning_part(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        let Some(r) = &mut self.reasoning else {
            return;
        };

        let Some(index) = r.part.take() else {
            return;
        };

        let item_id = Some(r.id.clone());
        let output_index = r.out;

        let text = match self.assembler.item(output_index) {
            Some(OutputItem::Reasoning {
                summary, content, ..
            }) => match self.to {
                ResponsesReasoningPosition::Content => content
                    .as_ref()
                    .and_then(|c| c.get(index as usize))
                    .and_then(ReasoningPart::text),
                _ => summary.get(index as usize).and_then(ReasoningPart::text),
            }
            .map(str::to_owned),
            _ => None,
        }
        .unwrap_or_default();

        match self.to {
            ResponsesReasoningPosition::Content => {
                self.emit(
                    ResponsesStreamEvent::ReasoningTextDone {
                        item_id: item_id.clone(),
                        output_index,
                        content_index: index,
                        text: text.clone(),
                        sequence_number: None,
                        additional_properties: Default::default(),
                    },
                    out,
                );
                self.emit(
                    ResponsesStreamEvent::ContentPartDone {
                        item_id,
                        output_index,
                        content_index: index,
                        part: ContentPart::reasoning_text(text),
                        sequence_number: None,
                        additional_properties: Default::default(),
                    },
                    out,
                );
            }
            _ => {
                self.emit(
                    ResponsesStreamEvent::ReasoningSummaryTextDone {
                        item_id: item_id.clone(),
                        output_index,
                        summary_index: index,
                        text: text.clone(),
                        sequence_number: None,
                        additional_properties: Default::default(),
                    },
                    out,
                );
                self.emit(
                    ResponsesStreamEvent::ReasoningSummaryPartDone {
                        item_id,
                        output_index,
                        summary_index: index,
                        part: ReasoningPart::summary_text(text),
                        sequence_number: None,
                        additional_properties: Default::default(),
                    },
                    out,
                );
            }
        }
    }

    fn close_reasoning(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        if self.reasoning.is_none() {
            return;
        }

        self.close_reasoning_part(out);
        let r = self.reasoning.take().unwrap();

        let item = self
            .assembler
            .item(r.out)
            .cloned()
            .unwrap_or_else(|| OutputItem::reasoning(Some(r.id.clone())));

        self.emit(
            ResponsesStreamEvent::OutputItemDone {
                output_index: r.out,
                item,
                sequence_number: None,
                additional_properties: Default::default(),
            },
            out,
        );
    }

    fn flush_pending(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        if self.message.as_ref().is_some_and(|m| m.pending_close) {
            self.close_message(out);
        }
    }

    fn flush_all(&mut self, out: &mut Vec<ResponsesStreamEvent>) {
        self.close_reasoning(out);
        self.close_message(out);
    }

    fn passthrough(
        &mut self,
        mut event: ResponsesStreamEvent,
        out: &mut Vec<ResponsesStreamEvent>,
    ) {
        match &mut event {
            ResponsesStreamEvent::OutputItemAdded { output_index, .. } => {
                self.flush_pending(out);
                let in_index = *output_index;
                let out_index = self.next_out;
                self.next_out += 1;
                self.item_map.insert(in_index, out_index);
                event.set_output_index(out_index);
            }
            ResponsesStreamEvent::Completed { response, .. }
            | ResponsesStreamEvent::Incomplete { response, .. }
            | ResponsesStreamEvent::Failed { response, .. } => {
                self.flush_all(out);
                response.output = self.assembler.output();
            }
            _ => {
                if let Some(in_index) = event.output_index() {
                    let out_index = self.item_map.get(&in_index).copied().unwrap_or(in_index);
                    event.set_output_index(out_index);
                }
            }
        }

        self.emit(event, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn ev(v: Value) -> ResponsesStreamEvent {
        serde_json::from_value(v).unwrap()
    }

    fn tags() -> ResponsesReasoningPosition {
        ResponsesReasoningPosition::text_unchecked("<think>", "</think>")
    }

    fn run(
        config: (ResponsesReasoningPosition, ResponsesReasoningPosition),
        input: Vec<Value>,
    ) -> Vec<Value> {
        let mut state = ResponsesReasoningRemappingState::new(config);
        let mut out = vec![];
        for v in input {
            out.extend(state.apply(ev(v)));
        }
        out.extend(state.flush());
        out.into_iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect()
    }

    fn seq(mut v: Value, n: u64) -> Value {
        v["sequence_number"] = json!(n);
        v
    }

    fn message(id: &str, status: &str, text: Option<&str>) -> Value {
        let content = match text {
            Some(t) => json!([{"type": "output_text", "text": t, "annotations": []}]),
            None => json!([]),
        };
        json!({"type": "message", "id": id, "role": "assistant", "status": status, "content": content})
    }

    fn reasoning(id: &str, summaries: &[&str]) -> Value {
        let summary: Vec<Value> = summaries
            .iter()
            .map(|s| json!({"type": "summary_text", "text": s}))
            .collect();
        json!({"type": "reasoning", "id": id, "summary": summary})
    }

    fn reasoning_with_content(id: &str, contents: &[&str]) -> Value {
        let content: Vec<Value> = contents
            .iter()
            .map(|s| json!({"type": "reasoning_text", "text": s}))
            .collect();
        json!({"type": "reasoning", "id": id, "summary": [], "content": content})
    }

    fn function_call(id: &str, args: &str) -> Value {
        json!({"type": "function_call", "id": id, "call_id": "c1", "name": "f", "arguments": args, "status": "completed"})
    }

    fn item_added(i: u32, item: Value) -> Value {
        json!({"type": "response.output_item.added", "output_index": i, "item": item})
    }
    fn item_done(i: u32, item: Value) -> Value {
        json!({"type": "response.output_item.done", "output_index": i, "item": item})
    }
    fn part_added(id: &str, i: u32, ci: u32) -> Value {
        json!({"type": "response.content_part.added", "item_id": id, "output_index": i, "content_index": ci, "part": {"type": "output_text", "text": "", "annotations": []}})
    }
    fn part_done(id: &str, i: u32, ci: u32, text: &str) -> Value {
        json!({"type": "response.content_part.done", "item_id": id, "output_index": i, "content_index": ci, "part": {"type": "output_text", "text": text, "annotations": []}})
    }
    fn text_delta(id: &str, i: u32, ci: u32, d: &str) -> Value {
        json!({"type": "response.output_text.delta", "item_id": id, "output_index": i, "content_index": ci, "delta": d, "logprobs": []})
    }
    fn text_done(id: &str, i: u32, ci: u32, t: &str) -> Value {
        json!({"type": "response.output_text.done", "item_id": id, "output_index": i, "content_index": ci, "text": t, "logprobs": []})
    }
    fn summary_part_added(id: &str, i: u32, si: u32) -> Value {
        json!({"type": "response.reasoning_summary_part.added", "item_id": id, "output_index": i, "summary_index": si, "part": {"type": "summary_text", "text": ""}})
    }
    fn summary_part_done(id: &str, i: u32, si: u32, t: &str) -> Value {
        json!({"type": "response.reasoning_summary_part.done", "item_id": id, "output_index": i, "summary_index": si, "part": {"type": "summary_text", "text": t}})
    }
    fn summary_delta(id: &str, i: u32, si: u32, d: &str) -> Value {
        json!({"type": "response.reasoning_summary_text.delta", "item_id": id, "output_index": i, "summary_index": si, "delta": d})
    }
    fn summary_done(id: &str, i: u32, si: u32, t: &str) -> Value {
        json!({"type": "response.reasoning_summary_text.done", "item_id": id, "output_index": i, "summary_index": si, "text": t})
    }
    fn reasoning_part_added(id: &str, i: u32, ci: u32) -> Value {
        json!({"type": "response.content_part.added", "item_id": id, "output_index": i, "content_index": ci, "part": {"type": "reasoning_text", "text": ""}})
    }
    fn reasoning_part_done(id: &str, i: u32, ci: u32, t: &str) -> Value {
        json!({"type": "response.content_part.done", "item_id": id, "output_index": i, "content_index": ci, "part": {"type": "reasoning_text", "text": t}})
    }
    fn reasoning_delta(id: &str, i: u32, ci: u32, d: &str) -> Value {
        json!({"type": "response.reasoning_text.delta", "item_id": id, "output_index": i, "content_index": ci, "delta": d})
    }
    fn reasoning_done(id: &str, i: u32, ci: u32, t: &str) -> Value {
        json!({"type": "response.reasoning_text.done", "item_id": id, "output_index": i, "content_index": ci, "text": t})
    }
    fn args_delta(id: &str, i: u32, d: &str) -> Value {
        json!({"type": "response.function_call_arguments.delta", "item_id": id, "output_index": i, "delta": d})
    }
    fn completed(output: Vec<Value>) -> Value {
        json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed", "output": output, "usage": {"input_tokens": 1, "output_tokens": 2, "total_tokens": 3}}})
    }

    fn native_reasoning_then_message(seqs: bool) -> Vec<Value> {
        let events = vec![
            json!({"type": "response.created", "response": {"id": "resp_1", "status": "in_progress", "output": []}}),
            item_added(0, reasoning("rs_1", &[])),
            summary_part_added("rs_1", 0, 0),
            summary_delta("rs_1", 0, 0, "pl"),
            summary_delta("rs_1", 0, 0, "an"),
            summary_done("rs_1", 0, 0, "plan"),
            summary_part_done("rs_1", 0, 0, "plan"),
            item_done(0, reasoning("rs_1", &["plan"])),
            item_added(1, message("msg_1", "in_progress", None)),
            part_added("msg_1", 1, 0),
            text_delta("msg_1", 1, 0, "answer"),
            text_done("msg_1", 1, 0, "answer"),
            part_done("msg_1", 1, 0, "answer"),
            item_done(1, message("msg_1", "completed", Some("answer"))),
            completed(vec![
                reasoning("rs_1", &["plan"]),
                message("msg_1", "completed", Some("answer")),
            ]),
        ];
        if seqs {
            events
                .into_iter()
                .enumerate()
                .map(|(i, e)| seq(e, i as u64 + 10))
                .collect()
        } else {
            events
        }
    }

    #[test]
    fn summary_becomes_tags_merged_into_the_following_message() {
        let out = run(
            (ResponsesReasoningPosition::Summary, tags()),
            native_reasoning_then_message(true),
        );

        let text = "<think>plan</think>answer";
        let expected: Vec<Value> = vec![
            json!({"type": "response.created", "response": {"id": "resp_1", "status": "in_progress", "output": []}}),
            item_added(0, message("msg_remap_0", "in_progress", None)),
            part_added("msg_remap_0", 0, 0),
            text_delta("msg_remap_0", 0, 0, "<think>"),
            text_delta("msg_remap_0", 0, 0, "pl"),
            text_delta("msg_remap_0", 0, 0, "an"),
            text_delta("msg_remap_0", 0, 0, "</think>"),
            text_delta("msg_remap_0", 0, 0, "answer"),
            text_done("msg_remap_0", 0, 0, text),
            part_done("msg_remap_0", 0, 0, text),
            item_done(0, message("msg_remap_0", "completed", Some(text))),
            completed(vec![message("msg_remap_0", "completed", Some(text))]),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, e)| seq(e, i as u64 + 10))
        .collect();

        assert_eq!(out, expected);
    }

    #[test]
    fn tags_become_a_summary_reasoning_item_before_the_message() {
        let out = run(
            (tags(), ResponsesReasoningPosition::Summary),
            vec![
                item_added(0, message("msg_1", "in_progress", None)),
                part_added("msg_1", 0, 0),
                text_delta("msg_1", 0, 0, "<thi"),
                text_delta("msg_1", 0, 0, "nk>pl"),
                text_delta("msg_1", 0, 0, "an</think>ans"),
                text_delta("msg_1", 0, 0, "wer"),
                text_done("msg_1", 0, 0, "<think>plan</think>answer"),
                part_done("msg_1", 0, 0, "<think>plan</think>answer"),
                item_done(
                    0,
                    message("msg_1", "completed", Some("<think>plan</think>answer")),
                ),
                completed(vec![message(
                    "msg_1",
                    "completed",
                    Some("<think>plan</think>answer"),
                )]),
            ],
        );

        assert_eq!(
            out,
            vec![
                item_added(0, reasoning("rs_remap_0", &[])),
                summary_part_added("rs_remap_0", 0, 0),
                summary_delta("rs_remap_0", 0, 0, "pl"),
                summary_delta("rs_remap_0", 0, 0, "an"),
                summary_done("rs_remap_0", 0, 0, "plan"),
                summary_part_done("rs_remap_0", 0, 0, "plan"),
                item_done(0, reasoning("rs_remap_0", &["plan"])),
                item_added(1, message("msg_1", "in_progress", None)),
                part_added("msg_1", 1, 0),
                text_delta("msg_1", 1, 0, "ans"),
                text_delta("msg_1", 1, 0, "wer"),
                text_done("msg_1", 1, 0, "answer"),
                part_done("msg_1", 1, 0, "answer"),
                item_done(1, message("msg_1", "completed", Some("answer"))),
                completed(vec![
                    reasoning("rs_remap_0", &["plan"]),
                    message("msg_1", "completed", Some("answer"))
                ]),
            ]
        );
    }

    #[test]
    fn summary_parts_become_reasoning_content_parts_and_the_message_is_untouched() {
        let out = run(
            (
                ResponsesReasoningPosition::Summary,
                ResponsesReasoningPosition::Content,
            ),
            native_reasoning_then_message(false),
        );

        assert_eq!(
            out,
            vec![
                json!({"type": "response.created", "response": {"id": "resp_1", "status": "in_progress", "output": []}}),
                item_added(0, reasoning_with_content("rs_1", &[])),
                reasoning_part_added("rs_1", 0, 0),
                reasoning_delta("rs_1", 0, 0, "pl"),
                reasoning_delta("rs_1", 0, 0, "an"),
                reasoning_done("rs_1", 0, 0, "plan"),
                reasoning_part_done("rs_1", 0, 0, "plan"),
                item_done(0, reasoning_with_content("rs_1", &["plan"])),
                item_added(1, message("msg_1", "in_progress", None)),
                part_added("msg_1", 1, 0),
                text_delta("msg_1", 1, 0, "answer"),
                text_done("msg_1", 1, 0, "answer"),
                part_done("msg_1", 1, 0, "answer"),
                item_done(1, message("msg_1", "completed", Some("answer"))),
                completed(vec![
                    reasoning_with_content("rs_1", &["plan"]),
                    message("msg_1", "completed", Some("answer"))
                ]),
            ]
        );
    }

    #[test]
    fn text_without_tags_is_unchanged() {
        let input = vec![
            item_added(0, message("msg_1", "in_progress", None)),
            part_added("msg_1", 0, 0),
            text_delta("msg_1", 0, 0, "plain"),
            text_done("msg_1", 0, 0, "plain"),
            part_done("msg_1", 0, 0, "plain"),
            item_done(0, message("msg_1", "completed", Some("plain"))),
            completed(vec![message("msg_1", "completed", Some("plain"))]),
        ];
        assert_eq!(
            run((tags(), ResponsesReasoningPosition::Summary), input.clone()),
            input
        );
    }

    #[test]
    fn other_items_are_reindexed_and_the_merged_message_is_closed_before_them() {
        let out = run(
            (ResponsesReasoningPosition::Summary, tags()),
            vec![
                item_added(0, reasoning("rs_1", &[])),
                summary_part_added("rs_1", 0, 0),
                summary_delta("rs_1", 0, 0, "plan"),
                summary_part_done("rs_1", 0, 0, "plan"),
                item_done(0, reasoning("rs_1", &["plan"])),
                item_added(1, function_call("fc_1", "")),
                args_delta("fc_1", 1, "{}"),
                item_done(1, function_call("fc_1", "{}")),
                completed(vec![
                    reasoning("rs_1", &["plan"]),
                    function_call("fc_1", "{}"),
                ]),
            ],
        );

        let text = "<think>plan</think>";
        assert_eq!(
            out,
            vec![
                item_added(0, message("msg_remap_0", "in_progress", None)),
                part_added("msg_remap_0", 0, 0),
                text_delta("msg_remap_0", 0, 0, "<think>"),
                text_delta("msg_remap_0", 0, 0, "plan"),
                text_delta("msg_remap_0", 0, 0, "</think>"),
                text_done("msg_remap_0", 0, 0, text),
                part_done("msg_remap_0", 0, 0, text),
                item_done(0, message("msg_remap_0", "completed", Some(text))),
                item_added(1, function_call("fc_1", "")),
                args_delta("fc_1", 1, "{}"),
                item_done(1, function_call("fc_1", "{}")),
                completed(vec![
                    message("msg_remap_0", "completed", Some(text)),
                    function_call("fc_1", "{}")
                ]),
            ]
        );
    }

    #[test]
    fn multiple_summary_parts_are_joined_with_blank_lines_in_tags() {
        let out = run(
            (ResponsesReasoningPosition::Summary, tags()),
            vec![
                item_added(0, reasoning("rs_1", &[])),
                summary_part_added("rs_1", 0, 0),
                summary_delta("rs_1", 0, 0, "a"),
                summary_part_done("rs_1", 0, 0, "a"),
                summary_part_added("rs_1", 0, 1),
                summary_delta("rs_1", 0, 1, "b"),
                summary_part_done("rs_1", 0, 1, "b"),
                item_done(0, reasoning("rs_1", &["a", "b"])),
            ],
        );

        let deltas: String = out
            .iter()
            .filter(|e| e["type"] == "response.output_text.delta")
            .map(|e| e["delta"].as_str().unwrap())
            .collect();
        assert_eq!(deltas, "<think>a\n\nb</think>");
        assert_eq!(out.last().unwrap()["type"], "response.output_item.done");
    }

    #[test]
    fn text_before_the_tag_keeps_the_original_id_and_the_rest_is_synthesized() {
        let out = run(
            (tags(), ResponsesReasoningPosition::Content),
            vec![
                item_added(0, message("msg_1", "in_progress", None)),
                part_added("msg_1", 0, 0),
                text_delta("msg_1", 0, 0, "hello <think>x</think> bye"),
                item_done(
                    0,
                    message("msg_1", "completed", Some("hello <think>x</think> bye")),
                ),
            ],
        );

        assert_eq!(
            out,
            vec![
                item_added(0, message("msg_1", "in_progress", None)),
                part_added("msg_1", 0, 0),
                text_delta("msg_1", 0, 0, "hello "),
                text_done("msg_1", 0, 0, "hello "),
                part_done("msg_1", 0, 0, "hello "),
                item_done(0, message("msg_1", "completed", Some("hello "))),
                item_added(1, reasoning_with_content("rs_remap_0", &[])),
                reasoning_part_added("rs_remap_0", 1, 0),
                reasoning_delta("rs_remap_0", 1, 0, "x"),
                reasoning_done("rs_remap_0", 1, 0, "x"),
                reasoning_part_done("rs_remap_0", 1, 0, "x"),
                item_done(1, reasoning_with_content("rs_remap_0", &["x"])),
                item_added(2, message("msg_remap_0", "in_progress", None)),
                part_added("msg_remap_0", 2, 0),
                text_delta("msg_remap_0", 2, 0, " bye"),
                text_done("msg_remap_0", 2, 0, " bye"),
                part_done("msg_remap_0", 2, 0, " bye"),
                item_done(2, message("msg_remap_0", "completed", Some(" bye"))),
            ]
        );
    }

    #[test]
    fn done_only_items_without_deltas_are_still_remapped() {
        let out = run(
            (ResponsesReasoningPosition::Summary, tags()),
            vec![
                item_done(0, reasoning("rs_1", &["plan"])),
                item_done(1, message("msg_1", "completed", Some("answer"))),
            ],
        );

        let deltas: String = out
            .iter()
            .filter(|e| e["type"] == "response.output_text.delta")
            .map(|e| e["delta"].as_str().unwrap())
            .collect();
        assert_eq!(deltas, "<think>plan</think>answer");
    }
}
