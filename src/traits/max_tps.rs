use async_trait::async_trait;

#[async_trait]
pub trait MaxTps: Send + Sync {
    async fn get(&self) -> Option<f32>;
}

#[async_trait]
impl MaxTps for f32 {
    async fn get(&self) -> Option<f32> {
        Some(*self)
    }
}

#[async_trait]
impl MaxTps for Option<&dyn MaxTps> {
    async fn get(&self) -> Option<f32> {
        if let Some(inner) = self {
            inner.get().await
        } else {
            None
        }
    }
}
