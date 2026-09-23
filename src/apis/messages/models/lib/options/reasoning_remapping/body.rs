use arrayvec::ArrayString;

use crate::apis::messages::models::{
    api::common::ContentBlock,
    lib::options::reasoning_remapping::{
        MessagesReasoningPosition, MessagesReasoningRemappingConfig,
    },
};

pub fn remap_content(
    content: Vec<ContentBlock>,
    config: &MessagesReasoningRemappingConfig,
) -> Vec<ContentBlock> {
    if config.from == config.to {
        return content;
    }

    let normalised = match &config.from {
        MessagesReasoningPosition::ThinkingBlock => content,
        MessagesReasoningPosition::Text {
            start_tag,
            stop_tag,
        } => extract_tags(content, start_tag, stop_tag),
    };

    match &config.to {
        MessagesReasoningPosition::ThinkingBlock => normalised,
        MessagesReasoningPosition::Text {
            start_tag,
            stop_tag,
        } => emit_tags(normalised, start_tag, stop_tag),
    }
}

fn extract_tags(
    content: Vec<ContentBlock>,
    start_tag: &ArrayString<32>,
    stop_tag: &ArrayString<32>,
) -> Vec<ContentBlock> {
    let mut out = Vec::with_capacity(content.len());

    for block in content {
        let (text, additional_properties) = match block {
            ContentBlock::Text {
                text,
                additional_properties,
            } => (text, additional_properties),
            other => {
                out.push(other);
                continue;
            }
        };

        let mut rest = text.as_str();

        while let Some(start) = rest.find(start_tag.as_str()) {
            let before = &rest[..start];
            let after_start = &rest[start + start_tag.len()..];

            if !before.is_empty() {
                out.push(ContentBlock::Text {
                    text: before.to_string(),
                    additional_properties: additional_properties.clone(),
                });
            }

            let (thinking, remaining) = match after_start.find(stop_tag.as_str()) {
                Some(stop) => (&after_start[..stop], &after_start[stop + stop_tag.len()..]),
                None => (after_start, ""),
            };

            out.push(ContentBlock::thinking(thinking, ""));
            rest = remaining;
        }

        if !rest.is_empty() {
            out.push(ContentBlock::Text {
                text: rest.to_string(),
                additional_properties,
            });
        }
    }

    out
}

fn emit_tags(
    content: Vec<ContentBlock>,
    start_tag: &ArrayString<32>,
    stop_tag: &ArrayString<32>,
) -> Vec<ContentBlock> {
    let mut out = Vec::with_capacity(content.len());
    let mut pending = String::new();

    for block in content {
        match block {
            ContentBlock::Thinking { thinking, .. } => {
                pending.push_str(start_tag);
                pending.push_str(&thinking);
                pending.push_str(stop_tag);
            }
            ContentBlock::Text {
                text,
                additional_properties,
            } => {
                out.push(ContentBlock::Text {
                    text: format!("{}{text}", std::mem::take(&mut pending)),
                    additional_properties,
                });
            }
            other => {
                if !pending.is_empty() {
                    out.push(ContentBlock::text(std::mem::take(&mut pending)));
                }
                out.push(other);
            }
        }
    }

    if !pending.is_empty() {
        out.push(ContentBlock::text(pending));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apis::messages::models::lib::options::reasoning_remapping::MessagesReasoningPosition;
    use serde_json::json;

    fn tags() -> MessagesReasoningPosition {
        MessagesReasoningPosition::text_unchecked("<think>", "</think>")
    }

    #[test]
    fn tags_become_a_thinking_block() {
        let out = remap_content(
            vec![ContentBlock::text("<think>plan</think>answer")],
            &(tags(), MessagesReasoningPosition::ThinkingBlock).into(),
        );
        assert_eq!(
            out,
            [
                ContentBlock::thinking("plan", ""),
                ContentBlock::text("answer")
            ]
        );
    }

    #[test]
    fn text_before_the_tag_and_an_unterminated_tag_are_handled() {
        let out = remap_content(
            vec![ContentBlock::text("pre <think>plan")],
            &(tags(), MessagesReasoningPosition::ThinkingBlock).into(),
        );
        assert_eq!(
            out,
            [
                ContentBlock::text("pre "),
                ContentBlock::thinking("plan", "")
            ]
        );
    }

    #[test]
    fn a_thinking_block_becomes_tags_in_the_following_text() {
        let out = remap_content(
            vec![
                ContentBlock::thinking("plan", "sig"),
                ContentBlock::text("answer"),
            ],
            &(MessagesReasoningPosition::ThinkingBlock, tags()).into(),
        );
        assert_eq!(out, [ContentBlock::text("<think>plan</think>answer")]);
    }

    #[test]
    fn a_thinking_block_with_no_following_text_becomes_its_own_text_block() {
        let tool = ContentBlock::ToolUse {
            name: "t".into(),
            input: json!({}),
            additional_properties: Default::default(),
        };
        let out = remap_content(
            vec![ContentBlock::thinking("plan", ""), tool.clone()],
            &(MessagesReasoningPosition::ThinkingBlock, tags()).into(),
        );
        assert_eq!(out, [ContentBlock::text("<think>plan</think>"), tool]);
    }

    #[test]
    fn same_position_is_identity_and_other_blocks_pass_through() {
        let blocks = vec![
            ContentBlock::Other(json!({"type": "image", "source": {}})),
            ContentBlock::text("<think>x</think>y"),
        ];
        let out = remap_content(blocks.clone(), &(tags(), tags()).into());
        assert_eq!(out, blocks);
    }

    #[test]
    fn tags_can_be_swapped_for_other_tags() {
        let out = remap_content(
            vec![ContentBlock::text("<think>plan</think>answer")],
            &(
                tags(),
                MessagesReasoningPosition::text_unchecked("[r]", "[/r]"),
            )
                .into(),
        );
        assert_eq!(out, [ContentBlock::text("[r]plan[/r]answer")]);
    }
}
