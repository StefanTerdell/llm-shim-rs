use crate::{stats::ChunkStats, traits::max_tps::MaxTps};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct TpsThrottler<'a> {
    max_tps: Option<&'a dyn MaxTps>,
    last: Instant,
    correction_secs: f32,
}

impl<'a> TpsThrottler<'a> {
    pub fn new(max_tps: Option<&'a dyn MaxTps>, started: Instant) -> Self {
        Self {
            max_tps,
            last: started,
            correction_secs: 0.0,
        }
    }

    pub async fn pace(&mut self, tokens: u32) -> ChunkStats {
        let now = Instant::now();
        let mut duration = now - self.last;
        self.last = now;

        if let Some(max_tps) = self.max_tps.get().await
            && let tokens_f = tokens as f32
            && let secs = duration.as_secs_f32()
            && secs > 0.0
            && tokens_f / secs > max_tps
        {
            self.correction_secs += tokens_f / max_tps - secs;
        }

        let tps_correction_duration = if self.correction_secs > 0.0 {
            let correction = Duration::from_secs_f32(self.correction_secs);
            sleep(correction).await;
            duration += correction;
            self.last = Instant::now();
            self.correction_secs = 0.0;

            Some(correction)
        } else {
            None
        };

        ChunkStats {
            duration,
            tokens,
            tps_correction_duration,
        }
    }
}

pub async fn pace_once<T, E, F>(
    max_tps: Option<&dyn MaxTps>,
    request: F,
    tokens: impl FnOnce(&T) -> u32,
) -> Result<(T, ChunkStats), E>
where
    F: Future<Output = Result<T, E>>,
{
    let before = MaxTps::get(&max_tps).await;
    let started = Instant::now();
    let value = request.await?;
    let after = MaxTps::get(&max_tps).await;
    let mut duration = started.elapsed();
    let tokens = tokens(&value);

    let max_tps = match (before, after) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        (a, b) => a.or(b),
    };

    let tps_correction_duration = max_tps.and_then(|max_tps| {
        let needed = tokens as f32 / max_tps;
        let secs = duration.as_secs_f32();

        (needed > secs).then(|| Duration::from_secs_f32(needed - secs))
    });

    if let Some(correction) = tps_correction_duration {
        sleep(correction).await;
        duration += correction;
    }

    Ok((
        value,
        ChunkStats {
            duration,
            tokens,
            tps_correction_duration,
        },
    ))
}
