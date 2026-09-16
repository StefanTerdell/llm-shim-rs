use std::{convert::Infallible, ops::Add, str::FromStr};

use indexmap::IndexMap;
use serde_json::Value;
use stefans_utils::literal_str;

use crate::{
    apis::chat_completion::models::api::common::CommonChatCompletionMessage, traits::or_add::OrAdd,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonChatCompletionRequestBody {
    pub model: Option<String>,
    pub messages: Vec<ChatCompletionRequestMessage>,
    pub logprobs: Option<bool>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionRequestMessage {
    #[serialize_always]
    pub content: Option<ChatCompletionRequestMessageContent>,
    #[serde(flatten)]
    pub common: CommonChatCompletionMessage,
}

impl Add for ChatCompletionRequestMessage {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            content: self.content.or_add(rhs.content),
            common: self.common + rhs.common,
        }
    }
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ChatCompletionRequestMessageContent {
    Text(String),
    Parts(Vec<ChatCompletionRequestMessageContentPart>),
}

impl core::iter::Sum for ChatCompletionRequestMessageContent {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::Parts(vec![]), |p, c| p + c)
    }
}
impl Add for ChatCompletionRequestMessageContent {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        match self {
            Self::Text(text) => match rhs {
                Self::Text(rhs_text) => Self::Text(format!("{text}{rhs_text}")),
                Self::Parts(mut vec) => {
                    if vec.is_empty() {
                        Self::Text(text)
                    } else {
                        vec.insert(0, text.into());
                        Self::Parts(vec)
                    }
                }
            },
            Self::Parts(mut vec) => match rhs {
                Self::Text(text) => {
                    if vec.is_empty() {
                        Self::Text(text)
                    } else {
                        vec.push(text.into());
                        Self::Parts(vec)
                    }
                }
                Self::Parts(mut rhs_vec) => {
                    vec.append(&mut rhs_vec);
                    Self::Parts(vec)
                }
            },
        }
    }
}

literal_str!(ChatCompletionRequestMessageContentPartTextType = "text");

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ChatCompletionRequestMessageContentPart {
    Text {
        r#type: ChatCompletionRequestMessageContentPartTextType,
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl From<String> for ChatCompletionRequestMessageContentPart {
    fn from(value: String) -> Self {
        Self::Text {
            text: value,
            r#type: Default::default(),
            additional_properties: Default::default(),
        }
    }
}

impl FromStr for ChatCompletionRequestMessageContentPart {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::Text {
            r#type: Default::default(),
            text: s.to_string(),
            additional_properties: Default::default(),
        })
    }
}
