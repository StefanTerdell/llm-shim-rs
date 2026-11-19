use indexmap::IndexMap;

use crate::apis::transcriptions::models::api::{
    request::TranscriptionsResponseFormat,
    response::non_streaming::{TranscriptionsJsonBody, TranscriptionsResponseBody},
};

pub fn render_verbose_json(
    body: TranscriptionsJsonBody,
    format: TranscriptionsResponseFormat,
) -> TranscriptionsResponseBody {
    let usage = body.usage_or_duration();

    match format {
        TranscriptionsResponseFormat::Json => {
            TranscriptionsResponseBody::Json(TranscriptionsJsonBody {
                id: body.id,
                text: body.text,
                usage,
                duration: None,
                language: None,
                segments: None,
                additional_properties: IndexMap::new(),
            })
        }
        TranscriptionsResponseFormat::VerboseJson | TranscriptionsResponseFormat::DiarizedJson => {
            TranscriptionsResponseBody::Json(TranscriptionsJsonBody { usage, ..body })
        }
        TranscriptionsResponseFormat::Text => TranscriptionsResponseBody::Text(body.text),
        TranscriptionsResponseFormat::Srt => TranscriptionsResponseBody::Text(
            cues(&body)
                .enumerate()
                .map(|(index, (start, end, text))| {
                    format!(
                        "{}\n{} --> {}\n{text}\n\n",
                        index + 1,
                        timestamp(start, ','),
                        timestamp(end, ',')
                    )
                })
                .collect(),
        ),
        TranscriptionsResponseFormat::Vtt => TranscriptionsResponseBody::Text(cues(&body).fold(
            "WEBVTT\n\n".to_string(),
            |mut vtt, (start, end, text)| {
                vtt.push_str(&format!(
                    "{} --> {}\n{text}\n\n",
                    timestamp(start, '.'),
                    timestamp(end, '.')
                ));
                vtt
            },
        )),
    }
}

fn cues(body: &TranscriptionsJsonBody) -> impl Iterator<Item = (f64, f64, &str)> {
    let segments = body.segments.as_deref().unwrap_or_default();
    let whole = segments
        .is_empty()
        .then(|| (0.0, body.duration.unwrap_or_default(), body.text.trim()));

    segments
        .iter()
        .map(|segment| (segment.start, segment.end, segment.text.trim()))
        .chain(whole)
}

fn timestamp(seconds: f64, separator: char) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;

    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}
