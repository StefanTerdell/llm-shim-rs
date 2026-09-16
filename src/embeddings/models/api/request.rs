use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct EmbeddingsRequestBody {
    pub model: String,
    pub input: EmbeddingsInput,
    pub encoding_format: Option<String>,
    pub dimensions: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum EmbeddingsInput {
    Text(String),
    Texts(Vec<String>),
    Tokens(Vec<u32>),
    TokenBatches(Vec<Vec<u32>>),
}
