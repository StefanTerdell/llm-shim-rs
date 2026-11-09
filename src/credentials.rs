use reqwest::{RequestBuilder, header::HeaderValue};
use std::fmt::Display;
use stefans_utils::secret::Secret;

/// How a request authenticates. Secret parts are wrapped in [`Secret`] so they
/// never show up in `Debug` output.
#[derive(Debug, Clone, PartialEq)]
pub enum Credentials {
    /// `Authorization: Bearer <token>`, as used by OpenAI-compatible servers.
    Bearer(Secret<String>),
    /// `Authorization: Basic base64(<username>:<password>)`.
    Basic {
        username: String,
        password: Option<Secret<String>>,
    },
    /// `x-api-key: <key>`, as used by Anthropic.
    XApiKey(Secret<String>),
    /// Any other header, such as `api-key` for Azure OpenAI. An invalid header
    /// name or value surfaces as an error when the request is sent.
    Header { name: String, value: Secret<String> },
}

impl Credentials {
    pub fn bearer<T: Display>(token: impl Into<Secret<T>>) -> Self {
        Self::Bearer(secret_string(token))
    }

    pub fn basic<T: Display>(
        username: impl Into<String>,
        password: Option<impl Into<Secret<T>>>,
    ) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.map(secret_string),
        }
    }

    pub fn x_api_key<T: Display>(key: impl Into<Secret<T>>) -> Self {
        Self::XApiKey(secret_string(key))
    }

    pub fn header<T: Display>(name: impl Into<String>, value: impl Into<Secret<T>>) -> Self {
        Self::Header {
            name: name.into(),
            value: secret_string(value),
        }
    }

    /// Adds the credentials to `request`, marking the header as sensitive.
    pub fn apply(self, request: RequestBuilder) -> RequestBuilder {
        match self {
            Self::Bearer(token) => request.bearer_auth(token.expose()),
            Self::Basic { username, password } => {
                request.basic_auth(username, password.map(Secret::expose))
            }
            Self::XApiKey(key) => sensitive_header(request, "x-api-key".to_string(), key),
            Self::Header { name, value } => sensitive_header(request, name, value),
        }
    }
}

fn secret_string<T: Display>(value: impl Into<Secret<T>>) -> Secret<String> {
    value.into().expose_to_string().into()
}

/// Sets a header flagged sensitive (as reqwest does for bearer and basic auth),
/// so it is redacted from reqwest's own debug output. An invalid value is
/// passed on unchanged so reqwest reports it when the request is sent.
fn sensitive_header(
    request: RequestBuilder,
    name: String,
    value: Secret<String>,
) -> RequestBuilder {
    let value = value.expose();

    match HeaderValue::try_from(value.as_str()) {
        Ok(mut header) => {
            header.set_sensitive(true);
            request.header(name, header)
        }
        Err(_) => request.header(name, value),
    }
}
