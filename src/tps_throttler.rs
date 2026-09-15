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
