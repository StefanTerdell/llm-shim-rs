use crate::{stats::ChunkStats, traits::max_tps::MaxTps};
use std::time::{Duration, Instant};
use tokio::{sync::Mutex, time::sleep};

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

pub const DEFAULT_MAX_TPS_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

pub async fn pace_once<T, E, F>(
    max_tps: Option<&dyn MaxTps>,
    sample_interval: Option<Duration>,
    request: F,
    tokens: impl FnOnce(&T) -> u32,
) -> Result<(T, ChunkStats), E>
where
    F: Future<Output = Result<T, E>>,
{
    let samples = Mutex::new(Samples::default());

    if let Some(sample) = MaxTps::get(&max_tps).await {
        samples.lock().await.push(sample);
    }

    let started = Instant::now();

    let value = tokio::select! {
        result = request => result?,
        _ = sample_max_tps(max_tps, sample_interval, &samples) => unreachable!("sampler never completes"),
    };

    let mut duration = started.elapsed();
    let tokens = tokens(&value);
    let max_tps = samples.into_inner().mean();

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

#[derive(Default)]
struct Samples {
    sum: f64,
    count: u32,
}

impl Samples {
    fn push(&mut self, sample: f32) {
        self.sum += sample as f64;
        self.count += 1;
    }

    fn mean(&self) -> Option<f32> {
        (self.count > 0).then(|| (self.sum / self.count as f64) as f32)
    }
}

async fn sample_max_tps(
    max_tps: Option<&dyn MaxTps>,
    interval: Option<Duration>,
    samples: &Mutex<Samples>,
) {
    let (Some(interval), Some(max_tps)) = (interval, max_tps) else {
        std::future::pending::<()>().await;
        unreachable!()
    };

    if interval.is_zero() {
        loop {
            if let Some(sample) = max_tps.get().await {
                samples.lock().await.push(sample);
            }

            tokio::task::yield_now().await;
        }
    }

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;

    loop {
        ticker.tick().await;

        if let Some(sample) = max_tps.get().await {
            samples.lock().await.push(sample);
        }
    }
}
