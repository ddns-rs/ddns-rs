use std::net::IpAddr;

use anyhow::{bail, Result};
use async_trait::async_trait;
use pnet::datalink;

use super::Interface;
use crate::IpType;

pub struct Stock {
    name: String,
}

impl Stock {
    pub fn create<N: AsRef<str>>(name: N) -> Result<Stock> {
        Ok(Stock { name: name.as_ref().to_owned() })
    }
}

#[async_trait]
impl Interface for Stock {
    async fn get_ip(&self, family: IpType) -> Result<Vec<IpAddr>> {
        if let Some(interface) = datalink::interfaces().into_iter().find(|interface| interface.name == self.name) {
            let result = interface
                .ips
                .into_iter()
                .map(|ip| ip.ip())
                .filter(IpAddr::is_global)
                .filter(|ip| {
                    if family == IpType::V4 && ip.is_ipv4() {
                        return true;
                    }
                    if family == IpType::V6 && ip.is_ipv6() {
                        return true;
                    }
                    false
                })
                .collect::<Vec<IpAddr>>();
            if !result.is_empty() {
                return Ok(result);
            }
            bail!("can't find global address for {}", family)
        } else {
            bail!("can't find except interface")
        }
    }
}
