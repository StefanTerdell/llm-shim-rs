use indexmap::IndexMap;
use serde_json::Value;

#[derive(..ApiModel)]
pub struct EmbeddingsRequestBody {
    pub input: EmbeddingsInput,
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
