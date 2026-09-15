use std::{
    ops::Add,
    time::{Duration, Instant},
};

use crate::traits::or_add::OrAdd;

#[derive(Debug, Clone)]
pub struct StreamStats {
    pub requested: Instant,
    pub chunks: Vec<ChunkStats>,
    pub input_tokens_is_estimate: bool,
    pub input_tokens: u32,
    pub output_tokens_is_estimate: bool,
    pub output_tokens: u32,
    pub error: Option<String>,
}

impl StreamStats {
    pub fn new(requested: Instant, input_tokens_estimate: u32) -> Self {
        Self {
            requested,
            chunks: Default::default(),
            input_tokens_is_estimate: true,
            input_tokens: input_tokens_estimate,
            output_tokens_is_estimate: true,
            output_tokens: 0,
            error: None,
        }
    }

    pub fn latency(&self) -> Option<Duration> {
        self.chunks.first().map(|x| x.duration)
    }

    pub fn tps_avg(&self) -> Option<f32> {
        if self.chunks.is_empty() {
            None
        } else {
            Some(
                self.chunks
                    .iter()
                    .fold(ChunkStats::default(), |p, c| p + *c)
                    .tps(),
            )
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChunkStats {
    pub duration: Duration,
    pub tokens: u32,
    pub tps_correction_duration: Option<Duration>,
}

impl ChunkStats {
    pub fn tps(&self) -> f32 {
        let secs = self.duration.as_secs_f32();

        if secs == 0.0 {
            secs
        } else {
            self.tokens as f32 / secs
        }
    }
}

impl Add for ChunkStats {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            duration: self.duration + rhs.duration,
            tokens: self.tokens + rhs.tokens,
            tps_correction_duration: self
                .tps_correction_duration
                .or_add(rhs.tps_correction_duration),
        }
    }
}
