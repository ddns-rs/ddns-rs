use std::fmt::{Display, Formatter};
use std::net::IpAddr;
use std::sync::Arc;

use anyhow::{ensure, Result};
use async_trait::async_trait;
use cloudflare::endpoints::dns::{
    CreateDnsRecord,
    CreateDnsRecordParams,
    DeleteDnsRecord,
    DnsContent,
    ListDnsRecords,
    ListDnsRecordsParams,
    UpdateDnsRecord,
    UpdateDnsRecordParams,
};
use cloudflare::endpoints::zone::{self, ListZones, ListZonesParams};
use cloudflare::framework::async_api::{ApiClient, Client};
use cloudflare::framework::auth::Credentials;
use cloudflare::framework::{Environment, HttpApiClientConfig, SearchMatch};
use log::{debug, warn};

use super::Provider;
use crate::IpType;

#[derive(PartialOrd, Eq, PartialEq, Hash, Debug, Clone)]
pub struct DNSRecord {
    pub id: String,
    pub ip: IpAddr,
}

impl Display for DNSRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl AsRef<IpAddr> for DNSRecord {
    #[inline]
    fn as_ref(&self) -> &IpAddr {
        &self.ip
    }
}

pub struct Cloudflare {
    dns: String,
    api_client: Arc<Client>,
    zone_identifier: String,
}

impl Cloudflare {
    pub async fn create<T: AsRef<str>, D: AsRef<str>>(token: T, dns: D) -> Result<Self> {
        let token = token.as_ref();
        let dns = dns.as_ref();
        let api_client = Arc::new(Client::new(
            Credentials::UserAuthToken {
                token: token.to_owned(),
            },
            HttpApiClientConfig::default(),
            Environment::Production,
        )?);

        let zone_name = if dns.ends_with('.') {
            let mut v = dns.rsplit('.').skip(1).take(2).collect::<Vec<_>>();
            v.reverse();
            v.join(".")
        } else {
            let mut v = dns.rsplit('.').take(2).collect::<Vec<_>>();
            v.reverse();
            v.join(".")
        };

        debug!("zone name is {}", zone_name);

        let zone_result = api_client
            .request(&ListZones {
                params: ListZonesParams {
                    name: Some(zone_name.clone()),
                    status: Some(zone::Status::Active),
                    page: Some(1),
                    per_page: Some(50),
                    order: None,
                    direction: None,
                    search_match: None,
                },
            })
            .await?
            .result;

        ensure!(!zone_result.is_empty(), "can't find zone with {}", zone_name);
        if zone_result.len() > 1 {
            warn!("more than one zone")
        }

        Ok(Cloudflare {
            dns: dns.to_owned(),
            api_client,
            zone_identifier: zone_result[0].id.clone(),
        })
    }
}

#[async_trait]
impl Provider for Cloudflare {
    type DNSRecord = DNSRecord;

    async fn get_dns_record(&self, family: IpType) -> Result<Vec<Self::DNSRecord>> {
        let mut result = vec![];
        let mut current_page = 1;
        loop {
            let dns_result = self
                .api_client
                .request(&ListDnsRecords {
                    zone_identifier: &self.zone_identifier,
                    params: ListDnsRecordsParams {
                        record_type: None,
                        name: Some(self.dns.clone()),
                        page: Some(current_page),
                        per_page: Some(50),
                        order: None,
                        direction: None,
                        search_match: Some(SearchMatch::All),
                    },
                })
                .await?
                .result;

            if dns_result.is_empty() {
                break;
            }

            for dns in &dns_result {
                match (family, &dns.content) {
                    (
                        IpType::V6,
                        DnsContent::AAAA {
                            content: ip,
                        },
                    ) => {
                        result.push(DNSRecord {
                            id: dns.id.clone(),
                            ip: IpAddr::V6(*ip),
                        });
                    },
                    (
                        IpType::V4,
                        DnsContent::A {
                            content: ip,
                        },
                    ) => {
                        result.push(DNSRecord {
                            id: dns.id.clone(),
                            ip: IpAddr::V4(*ip),
                        });
                    },
                    _ => {},
                }
            }

            if dns_result.len() < 50 {
                break;
            }
            current_page += 1;
        }
        Ok(result)
    }

    async fn create_dns_record(&self, ip: &IpAddr, ttl: u32) -> Result<()> {
        let content = match *ip {
            IpAddr::V6(ip) => DnsContent::AAAA {
                content: ip,
            },
            IpAddr::V4(ip) => DnsContent::A {
                content: ip,
            },
        };
        self.api_client
            .request(&CreateDnsRecord {
                zone_identifier: &self.zone_identifier,
                params: CreateDnsRecordParams {
                    ttl: Some(ttl),
                    priority: None,
                    proxied: None,
                    name: &self.dns,
                    content,
                },
            })
            .await?;

        Ok(())
    }

    async fn update_dns_record(&self, record: &Self::DNSRecord, ip: &IpAddr) -> Result<()> {
        let content = match *ip {
            IpAddr::V6(ip) => DnsContent::AAAA {
                content: ip,
            },
            IpAddr::V4(ip) => DnsContent::A {
                content: ip,
            },
        };
        self.api_client
            .request(&UpdateDnsRecord {
                zone_identifier: &self.zone_identifier,
                identifier: &record.id,
                params: UpdateDnsRecordParams {
                    ttl: None,
                    proxied: None,
                    name: &self.dns,
                    content,
                },
            })
            .await?;

        Ok(())
    }

    async fn delete_dns_record(&self, record: &Self::DNSRecord) -> Result<()> {
        self.api_client
            .request(&DeleteDnsRecord {
                zone_identifier: &self.zone_identifier,
                identifier: &record.id,
            })
            .await?;

        Ok(())
    }
}
