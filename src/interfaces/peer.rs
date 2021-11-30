use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;

use super::Interface;
use crate::IpType;

pub struct Peer {
    url_v4: String,
    url_v6: String,
    client_v4: Client,
    client_v6: Client,
    ipv4_field_path: String,
    ipv6_field_path: String,
}

impl Peer {
    pub fn create<URL: AsRef<str>, P: AsRef<str>>(
        url_v4: URL,
        url_v6: URL,
        ipv4_field_path: P,
        ipv6_field_path: P,
    ) -> Result<Peer> {
        let client_v4 = reqwest::Client::builder().local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED)).build()?;
        let client_v6 = reqwest::Client::builder().local_address(IpAddr::V6(Ipv6Addr::UNSPECIFIED)).build()?;
        Ok(Peer {
            url_v4: url_v4.as_ref().to_owned(),
            url_v6: url_v6.as_ref().to_owned(),
            ipv4_field_path: ipv4_field_path.as_ref().to_owned(),
            ipv6_field_path: ipv6_field_path.as_ref().to_owned(),
            client_v4,
            client_v6,
        })
    }
}

#[async_trait]
impl Interface for Peer {
    async fn get_ip(&self, family: IpType) -> anyhow::Result<Vec<IpAddr>> {
        let (url, client, ip_field_path) = match family {
            IpType::V4 => (&*self.url_v4, &self.client_v4, &*self.ipv4_field_path),
            IpType::V6 => (&*self.url_v6, &self.client_v6, &*self.ipv6_field_path),
        };
        let parties = ip_field_path.splitn(2, ':').collect::<Vec<_>>();
        if parties.len() != 2 {
            bail!("ip field path illegal")
        }
        let mut ips = vec![];
        let result = client.get(url).send().await?;
        match parties[0] {
            "regex" => {
                let parties = parties[1].splitn(2, ':').collect::<Vec<_>>();
                if parties.len() != 2 {
                    bail!(r#"regex extractor format must be "capture_group_number:expression""#)
                }
                let index =
                    parties[0].parse::<usize>().map_err(|err| anyhow!("can't parse capture group index: {}", err))?;
                match Regex::new(parties[1]) {
                    Ok(re) => {
                        let result = result.text().await?;
                        let caps = re.captures(&result).ok_or_else(|| anyhow!("can't match"))?;
                        let content = caps.get(index).ok_or_else(|| anyhow!("can't get capture group {}", index))?;
                        ips.push(content.as_str().parse()?);
                    },
                    Err(_) => {
                        bail!("regex illegal {}", parties[1])
                    },
                }
            },
            "json" => {
                let result = result.json::<HashMap<String, String>>().await?;
                let result = path_value::to_value(result)?;
                let ip = result.get::<String, _, _>(ip_field_path)?.ok_or_else(|| anyhow!("can't get ip by peer"))?;
                ips.push(ip.parse()?);
            },
            _ => {
                bail!("unsupported extract method: {}", parties[0])
            },
        };
        Ok(ips)
    }
}
