use async_trait::async_trait;
use std::sync::Arc;

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
impl<T: MaxTps> MaxTps for Option<T> {
    async fn get(&self) -> Option<f32> {
        match self {
            Some(inner) => inner.get().await,
            None => None,
        }
    }
}

#[async_trait]
impl<T: MaxTps + ?Sized> MaxTps for &T {
    async fn get(&self) -> Option<f32> {
        (**self).get().await
    }
}

#[async_trait]
impl<T: MaxTps + ?Sized> MaxTps for Box<T> {
    async fn get(&self) -> Option<f32> {
        (**self).get().await
    }
}

#[async_trait]
impl<T: MaxTps + ?Sized> MaxTps for Arc<T> {
    async fn get(&self) -> Option<f32> {
        (**self).get().await
    }
}
