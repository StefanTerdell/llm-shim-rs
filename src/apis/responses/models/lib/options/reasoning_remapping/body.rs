use arrayvec::ArrayString;

use crate::apis::responses::models::{
    api::common::{ContentPart, OutputItem, ReasoningPart},
    lib::options::reasoning_remapping::{
        ResponsesReasoningPosition, ResponsesReasoningRemappingConfig,
    },
};

pub fn remap_output(
    output: Vec<OutputItem>,
    config: &ResponsesReasoningRemappingConfig,
) -> Vec<OutputItem> {
    if config.from == config.to {
        return output;
    }

    let normalised = match &config.from {
        ResponsesReasoningPosition::Summary => output,
        ResponsesReasoningPosition::Content => content_to_summary(output),
        ResponsesReasoningPosition::Text {
            start_tag,
            stop_tag,
        } => extract_tags(output, start_tag, stop_tag),
    };

    match &config.to {
        ResponsesReasoningPosition::Summary => normalised,
        ResponsesReasoningPosition::Content => summary_to_content(normalised),
        ResponsesReasoningPosition::Text {
            start_tag,
            stop_tag,
        } => emit_tags(normalised, start_tag, stop_tag),
    }
}

fn content_to_summary(output: Vec<OutputItem>) -> Vec<OutputItem> {
    output
        .into_iter()
        .map(|item| match item {
            OutputItem::Reasoning {
                id,
                mut summary,
                content: Some(content),
                additional_properties,
            } => {
                summary.extend(content.into_iter().map(|part| match part.text() {
                    Some(text) => ReasoningPart::summary_text(text),
                    None => part,
                }));
                OutputItem::Reasoning {
                    id,
                    summary,
                    content: None,
                    additional_properties,
                }
            }
            other => other,
        })
        .collect()
}

fn summary_to_content(output: Vec<OutputItem>) -> Vec<OutputItem> {
    output
        .into_iter()
        .map(|item| match item {
            OutputItem::Reasoning {
                id,
                summary,
                content,
                additional_properties,
            } => {
                let mut content = content.unwrap_or_default();
                content.extend(summary.into_iter().map(|part| match part.text() {
                    Some(text) => ReasoningPart::reasoning_text(text),
                    None => part,
                }));
                OutputItem::Reasoning {
                    id,
                    summary: vec![],
                    content: Some(content),
                    additional_properties,
                }
            }
            other => other,
        })
        .collect()
}

fn extract_tags(
    output: Vec<OutputItem>,
    start_tag: &ArrayString<32>,
    stop_tag: &ArrayString<32>,
) -> Vec<OutputItem> {
    let mut out = Vec::with_capacity(output.len());

    for item in output {
        let (id, role, status, content, additional_properties) = match item {
            OutputItem::Message {
                id,
                role,
                status,
                content,
                additional_properties,
            } => (id, role, status, content, additional_properties),
            other => {
                out.push(other);
                continue;
            }
        };

        let mut reasonings = Vec::new();
        let content = content
            .into_iter()
            .map(|part| match part {
                ContentPart::OutputText {
                    text,
                    additional_properties,
                } => ContentPart::OutputText {
                    text: strip_tags(&text, start_tag, stop_tag, &mut reasonings),
                    additional_properties,
                },
                other => other,
            })
            .collect();

        for reasoning in reasonings {
            let mut item = OutputItem::reasoning(None);
            if let OutputItem::Reasoning { summary, .. } = &mut item {
                summary.push(ReasoningPart::summary_text(reasoning));
            }
            out.push(item);
        }

        out.push(OutputItem::Message {
            id,
            role,
            status,
            content,
            additional_properties,
        });
    }

    out
}

fn strip_tags(
    text: &str,
    start_tag: &ArrayString<32>,
    stop_tag: &ArrayString<32>,
    reasonings: &mut Vec<String>,
) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find(start_tag.as_str()) {
        kept.push_str(&rest[..start]);
        let after_start = &rest[start + start_tag.len()..];

        match after_start.find(stop_tag.as_str()) {
            Some(stop) => {
                reasonings.push(after_start[..stop].to_string());
                rest = &after_start[stop + stop_tag.len()..];
            }
            None => {
                reasonings.push(after_start.to_string());
                rest = "";
            }
        }
    }

    kept.push_str(rest);
    kept
}

fn emit_tags(
    output: Vec<OutputItem>,
    start_tag: &ArrayString<32>,
    stop_tag: &ArrayString<32>,
) -> Vec<OutputItem> {
    let mut out = Vec::with_capacity(output.len());
    let mut pending = String::new();

    for item in output {
        match item {
            OutputItem::Reasoning {
                summary, content, ..
            } => {
                let joined = summary
                    .iter()
                    .chain(content.iter().flatten())
                    .filter_map(ReasoningPart::text)
                    .collect::<Vec<_>>()
                    .join("\n\n");
                pending.push_str(start_tag);
                pending.push_str(&joined);
                pending.push_str(stop_tag);
            }
            OutputItem::Message {
                id,
                role,
                status,
                mut content,
                additional_properties,
            } => {
                if !pending.is_empty() {
                    let rendered = std::mem::take(&mut pending);
                    match content.iter_mut().find_map(|part| match part {
                        ContentPart::OutputText { text, .. } => Some(text),
                        _ => None,
                    }) {
                        Some(text) => text.insert_str(0, &rendered),
                        None => content.insert(0, ContentPart::output_text(rendered)),
                    }
                }
                out.push(OutputItem::Message {
                    id,
                    role,
                    status,
                    content,
                    additional_properties,
                });
            }
            other => {
                flush_pending(&mut pending, &mut out);
                out.push(other);
            }
        }
    }

    flush_pending(&mut pending, &mut out);
    out
}

fn flush_pending(pending: &mut String, out: &mut Vec<OutputItem>) {
    if pending.is_empty() {
        return;
    }

    let mut message = OutputItem::message(None);
    if let OutputItem::Message { content, .. } = &mut message {
        content.push(ContentPart::output_text(std::mem::take(pending)));
    }
    out.push(message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apis::responses::models::lib::options::reasoning_remapping::ResponsesReasoningPosition;
    use serde_json::json;

    fn tags() -> ResponsesReasoningPosition {
        ResponsesReasoningPosition::text_unchecked("<think>", "</think>")
    }

    fn message(id: &str, text: &str) -> OutputItem {
        OutputItem::Message {
            id: Some(id.into()),
            role: Some("assistant".into()),
            status: Some("completed".into()),
            content: vec![ContentPart::output_text(text)],
            additional_properties: Default::default(),
        }
    }

    fn reasoning_with_summary(id: &str, texts: &[&str]) -> OutputItem {
        OutputItem::Reasoning {
            id: Some(id.into()),
            summary: texts
                .iter()
                .map(|t| ReasoningPart::summary_text(*t))
                .collect(),
            content: None,
            additional_properties: Default::default(),
        }
    }

    fn reasoning_with_content(id: &str, texts: &[&str]) -> OutputItem {
        OutputItem::Reasoning {
            id: Some(id.into()),
            summary: vec![],
            content: Some(
                texts
                    .iter()
                    .map(|t| ReasoningPart::reasoning_text(*t))
                    .collect(),
            ),
            additional_properties: Default::default(),
        }
    }

    #[test]
    fn tags_become_a_reasoning_item_before_the_message() {
        let out = remap_output(
            vec![message("msg_1", "<think>plan</think>answer")],
            &(tags(), ResponsesReasoningPosition::Summary).into(),
        );
        assert_eq!(out.len(), 2);
        let OutputItem::Reasoning { id, summary, .. } = &out[0] else {
            panic!("reasoning")
        };
        assert!(id.is_none());
        assert_eq!(summary, &[ReasoningPart::summary_text("plan")]);
        assert_eq!(out[1], message("msg_1", "answer"));
    }

    #[test]
    fn summary_becomes_tags_in_the_following_message() {
        let out = remap_output(
            vec![
                reasoning_with_summary("rs_1", &["plan"]),
                message("msg_1", "answer"),
            ],
            &(ResponsesReasoningPosition::Summary, tags()).into(),
        );
        assert_eq!(out, [message("msg_1", "<think>plan</think>answer")]);
    }

    #[test]
    fn summary_with_no_following_message_gets_a_synthetic_message() {
        let call = OutputItem::FunctionCall {
            id: Some("fc_1".into()),
            name: "f".into(),
            arguments: "{}".into(),
            additional_properties: Default::default(),
        };
        let out = remap_output(
            vec![reasoning_with_summary("rs_1", &["plan"]), call.clone()],
            &(ResponsesReasoningPosition::Summary, tags()).into(),
        );
        assert_eq!(out.len(), 2);
        let OutputItem::Message { id, content, .. } = &out[0] else {
            panic!("message")
        };
        assert!(id.is_none());
        assert_eq!(content, &[ContentPart::output_text("<think>plan</think>")]);
        assert_eq!(out[1], call);
    }

    #[test]
    fn summary_and_content_convert_both_ways() {
        let to_content = remap_output(
            vec![reasoning_with_summary("rs_1", &["a", "b"])],
            &(
                ResponsesReasoningPosition::Summary,
                ResponsesReasoningPosition::Content,
            )
                .into(),
        );
        assert_eq!(to_content, [reasoning_with_content("rs_1", &["a", "b"])]);

        let back = remap_output(
            to_content,
            &(
                ResponsesReasoningPosition::Content,
                ResponsesReasoningPosition::Summary,
            )
                .into(),
        );
        assert_eq!(back, [reasoning_with_summary("rs_1", &["a", "b"])]);
    }

    #[test]
    fn tags_can_go_straight_to_content() {
        let out = remap_output(
            vec![message("msg_1", "<think>plan</think>answer")],
            &(tags(), ResponsesReasoningPosition::Content).into(),
        );
        let OutputItem::Reasoning {
            summary, content, ..
        } = &out[0]
        else {
            panic!("reasoning")
        };
        assert!(summary.is_empty());
        assert_eq!(
            content.as_deref(),
            Some(&[ReasoningPart::reasoning_text("plan")][..])
        );
    }

    #[test]
    fn multi_part_summaries_are_joined_with_a_blank_line_like_the_stream() {
        let out = remap_output(
            vec![
                reasoning_with_summary("rs_1", &["a", "b"]),
                message("msg_1", "x"),
            ],
            &(ResponsesReasoningPosition::Summary, tags()).into(),
        );
        assert_eq!(out, [message("msg_1", "<think>a\n\nb</think>x")]);
    }

    #[test]
    fn content_only_reasoning_still_renders_into_tags() {
        let out = remap_output(
            vec![
                reasoning_with_content("rs_1", &["plan"]),
                message("msg_1", "x"),
            ],
            &(ResponsesReasoningPosition::Summary, tags()).into(),
        );
        assert_eq!(out, [message("msg_1", "<think>plan</think>x")]);
    }

    #[test]
    fn same_position_is_identity_and_other_items_pass_through() {
        let items = vec![
            OutputItem::Other(json!({"type": "web_search_call", "id": "ws_1"})),
            message("msg_1", "<think>x</think>y"),
        ];
        assert_eq!(remap_output(items.clone(), &(tags(), tags()).into()), items);
    }
}
