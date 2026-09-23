use crate::{
    apis::responses::models::{
        api::{
            request::non_streaming::NonStreamingResponsesRequestBody,
            response::non_streaming::{NonStreamingResponsesResponse, ResponseBody},
        },
        lib::options::{ResponsesOptions, reasoning_remapping::body::remap_output},
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub async fn non_streaming_responses<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingResponsesRequestBody>,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Result<NonStreamingResponsesResponse, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();

    let mut request = client.post(url).json(&body);

    if let Some(bearer_token) = options.bearer_token {
        request = request.bearer_auth(bearer_token.expose());
    }

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());

    let (mut body, chunk) = pace_once(
        options.max_tps,
        options.max_tps_sample_interval,
        async {
            Ok::<_, Error>(
                request
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<ResponseBody>()
                    .await?,
            )
        },
        output_tokens,
    )
    .await?;

    if let Some(Some(usage)) = &body.usage {
        if let Some(input_tokens) = usage.input_tokens {
            stats.input_tokens_is_estimate = false;
            stats.input_tokens = input_tokens;
        }

        if let Some(output_tokens) = usage.output_tokens {
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = output_tokens;
        }
    }

    if stats.output_tokens_is_estimate {
        stats.output_tokens = output_tokens(&body);
    }

    stats.chunks.push(chunk);

    if let Some(config) = &options.reasoning_remapping {
        body.output = remap_output(std::mem::take(&mut body.output), config);
    }

    let error = body.error.clone().flatten();

    if let Some(error) = &error {
        stats.error = Some(error.to_string());
    }

    let usage = body.usage.get_or_insert_default().get_or_insert_default();
    usage.input_tokens.get_or_insert(stats.input_tokens);
    usage.output_tokens.get_or_insert(stats.output_tokens);
    usage
        .total_tokens
        .get_or_insert(usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0));

    Ok(NonStreamingResponsesResponse {
        body,
        stats: Some(stats),
        error,
    })
}

fn output_tokens(body: &ResponseBody) -> u32 {
    body.usage
        .as_ref()
        .and_then(|usage| usage.as_ref())
        .and_then(|usage| usage.output_tokens)
        .unwrap_or_else(|| {
            body.output
                .iter()
                .map(EstimateTokens::estimate_tokens)
                .sum()
        })
}
