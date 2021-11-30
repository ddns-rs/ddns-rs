use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
pub use peer::Peer;
pub use stock::Stock;

use crate::IpType;

mod peer;
mod stock;

#[async_trait]
pub trait Interface: Send + Sync {
    async fn get_ip(&self, family: IpType) -> Result<Vec<IpAddr>>;
}
