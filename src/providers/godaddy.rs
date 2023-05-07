use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr};

use addr::parse_dns_name;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{record_type_from_ip, Provider};
use crate::IpType;

pub struct Credentials {
    pub api_key: String,
    pub secret: String,
}

#[derive(Deserialize, PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub struct DNSRecord {
    pub kind: String,
    pub domain: String,
    pub name: String,
    pub ttl: u64,
    pub ip: IpAddr,
}

impl Display for DNSRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.domain, self.ip)
    }
}

impl AsRef<IpAddr> for DNSRecord {
    #[inline]
    fn as_ref(&self) -> &IpAddr {
        &self.ip
    }
}

pub struct Godaddy {
    domain: String,
    name: String,
    client: Client,
    cred: Credentials,
}

impl Godaddy {
    pub async fn create<A: AsRef<str>, S: AsRef<str>, D: AsRef<str>>(api_key: A, secret: S, dns: D) -> Result<Self> {
        // current godaddy not support ipv6 so we force use ipv4
        let client = reqwest::Client::builder()
            .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .build()?;
        let api_key = api_key.as_ref().to_owned();
        let secret = secret.as_ref().to_owned();
        let dns = dns.as_ref().to_owned();

        let result = parse_dns_name(&dns).unwrap();
        let domain = result.root().unwrap_or("").to_owned();
        let name = dns.trim_end_matches(&domain).trim_end_matches('.').to_owned();

        Ok(Godaddy {
            domain,
            name,
            client,
            cred: Credentials {
                api_key,
                secret,
            },
        })
    }
}

#[async_trait]
impl Provider for Godaddy {
    type DNSRecord = DNSRecord;

    async fn get_dns_record(&self, family: IpType) -> Result<Vec<Self::DNSRecord>> {
        let mut records = vec![];
        let kind = match family {
            IpType::V4 => "A",
            IpType::V6 => "AAAA",
        };
        let url = format!(
            "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
            self.domain, kind, self.name
        );
        let result = self
            .client
            .get(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("sso-key {}:{}", self.cred.api_key, self.cred.secret),
            )
            .send()
            .await?
            .json::<Vec<HashMap<String, serde_json::Value>>>()
            .await?;
        for item in result {
            records.push(DNSRecord {
                kind: kind.to_owned(),
                domain: self.domain.clone(),
                name: self.name.clone(),
                ttl: item.get("ttl").unwrap().as_u64().unwrap(),
                ip: item.get("data").unwrap().as_str().unwrap().parse().unwrap(),
            })
        }
        Ok(records)
    }

    async fn create_dns_record(&self, ip: &IpAddr, ttl: u32) -> Result<()> {
        let url = format!("https://api.godaddy.com/v1/domains/{}/records", self.domain);
        let json = vec![json!({
            "data": ip.to_string(),
            "name": self.name,
            "type": record_type_from_ip(ip),
            "ttl": ttl,
        })];

        self.client
            .patch(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("sso-key {}:{}", self.cred.api_key, self.cred.secret),
            )
            .json(&json)
            .send()
            .await?;
        Ok(())
    }

    async fn update_dns_record(&self, record: &Self::DNSRecord, ip: &IpAddr) -> Result<()> {
        let json = vec![json!({
            "data": ip,
            "ttl": record.ttl,
        })];
        let url = format!(
            "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
            record.domain, record.kind, record.name
        );

        self.client
            .put(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("sso-key {}:{}", self.cred.api_key, self.cred.secret),
            )
            .json(&json)
            .send()
            .await?;
        Ok(())
    }

    async fn delete_dns_record(&self, record: &Self::DNSRecord) -> Result<()> {
        let url = format!(
            "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
            record.domain, record.kind, record.name
        );

        self.client
            .delete(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("sso-key {}:{}", self.cred.api_key, self.cred.secret),
            )
            .send()
            .await?;
        Ok(())
    }
}
