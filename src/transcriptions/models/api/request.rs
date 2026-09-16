use bytes::Bytes;
use indexmap::IndexMap;
use reqwest::{
    Body,
    multipart::{Form, Part},
};
use serde_json::Value;
use std::path::Path;
use tokio::fs::File;

use crate::error::Error;

#[derive(Debug)]
pub struct TranscriptionsRequest {
    pub file: AudioFile,
    pub fields: TranscriptionsFields,
}

impl TranscriptionsRequest {
    pub fn new(file: AudioFile, fields: impl Into<TranscriptionsFields>) -> Self {
        Self {
            file,
            fields: fields.into(),
        }
    }

    pub fn into_form(self, stream: Option<bool>) -> Result<Form, Error> {
        let mut form = Form::new().part("file", self.file.into_part()?);

        if let Some(stream) = stream.or(self.fields.stream) {
            form = form.text("stream", stream.to_string());
        }

        for (key, value) in self.fields.additional_properties {
            form = add_field(form, key, value);
        }

        Ok(form)
    }
}

fn add_field(form: Form, key: String, value: Value) -> Form {
    match value {
        Value::Null => form,
        Value::Array(items) => items.into_iter().fold(form, |form, item| {
            form.text(format!("{key}[]"), scalar(item))
        }),
        value => form.text(key, scalar(value)),
    }
}

fn scalar(value: Value) -> String {
    match value {
        Value::String(text) => text,
        other => other.to_string(),
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct TranscriptionsFields {
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(Debug)]
pub struct AudioFile {
    pub filename: String,
    pub mime: Option<String>,
    source: AudioSource,
}

#[derive(Debug)]
enum AudioSource {
    Bytes(Bytes),
    File { file: File, length: u64 },
    Stream { body: Body, length: Option<u64> },
}

impl AudioFile {
    pub fn bytes(filename: impl Into<String>, bytes: impl Into<Bytes>) -> Self {
        Self {
            filename: filename.into(),
            mime: None,
            source: AudioSource::Bytes(bytes.into()),
        }
    }

    pub async fn path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let file = File::open(path).await?;
        let length = file.metadata().await?.len();
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".to_string());

        Ok(Self {
            filename,
            mime: None,
            source: AudioSource::File { file, length },
        })
    }

    pub fn stream(filename: impl Into<String>, body: impl Into<Body>, length: Option<u64>) -> Self {
        Self {
            filename: filename.into(),
            mime: None,
            source: AudioSource::Stream {
                body: body.into(),
                length,
            },
        }
    }

    pub fn with_mime(mut self, mime: impl Into<String>) -> Self {
        self.mime = Some(mime.into());
        self
    }

    fn into_part(self) -> Result<Part, Error> {
        let mut part = match self.source {
            AudioSource::Bytes(bytes) => {
                let length = bytes.len() as u64;
                Part::stream_with_length(Body::from(bytes), length)
            }
            AudioSource::File { file, length } => {
                Part::stream_with_length(Body::from(file), length)
            }
            AudioSource::Stream {
                body,
                length: Some(length),
            } => Part::stream_with_length(body, length),
            AudioSource::Stream { body, length: None } => Part::stream(body),
        }
        .file_name(self.filename);

        if let Some(mime) = self.mime {
            part = part.mime_str(&mime)?;
        }

        Ok(part)
    }
}
