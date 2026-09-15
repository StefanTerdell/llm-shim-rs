use async_trait::async_trait;

#[async_trait]
pub trait TpsThrottler: Send + Sync {
    async fn get_max_tps(&self) -> Option<f32>;
}

#[async_trait]
impl TpsThrottler for Option<&dyn TpsThrottler> {
    async fn get_max_tps(&self) -> Option<f32> {
        if let Some(inner) = self {
            inner.get_max_tps().await
        } else {
            None
        }
    }
}
