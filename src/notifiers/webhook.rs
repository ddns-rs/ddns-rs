use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;

use crate::Notifier;

pub struct Webhook {
    url: String,
    authorization_header: String,
    client: Client,
}

impl Webhook {
    pub async fn create<S: AsRef<str>>(
        url: S,
        authorization_header: S,
        local_address: Option<IpAddr>,
    ) -> Result<Webhook> {
        let url = url.as_ref().to_owned();
        let authorization_header = authorization_header.as_ref().to_owned();
        let builder = reqwest::Client::builder();
        let client = if let Some(local_address) = local_address {
            builder.local_address(local_address).build()?
        } else {
            builder.build()?
        };
        Ok(Webhook {
            url,
            authorization_header,
            client,
        })
    }
}

#[async_trait]
impl Notifier for Webhook {
    async fn send(&self, new_ips: &[IpAddr]) -> anyhow::Result<()> {
        let url = &self.url;
        let ipv4_list = new_ips.iter().filter(|v| v.is_ipv4()).collect::<Vec<_>>();
        let ipv6_list = new_ips.iter().filter(|v| v.is_ipv6()).collect::<Vec<_>>();
        let json = vec![json!({
            "ipv4_list": ipv4_list,
            "ipv6_list": ipv6_list,
        })];

        self.client
            .post(url)
            .header(reqwest::header::AUTHORIZATION, &self.authorization_header)
            .json(&json)
            .send()
            .await?;
        Ok(())
    }
}
