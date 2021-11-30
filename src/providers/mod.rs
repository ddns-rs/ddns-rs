use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
use log::info;

pub use self::cloudflare::Cloudflare;
pub use self::fake::Fake;
pub use self::godaddy::Godaddy;
use crate::IpType;

mod cloudflare;
mod fake;
mod godaddy;

#[async_trait]
pub trait Provider: Send + Sync {
    type DNSRecord: AsRef<IpAddr> + Send + Sync + Eq + PartialEq;

    async fn get_dns_record(&self, family: IpType) -> Result<Vec<Self::DNSRecord>>;
    async fn create_dns_record(&self, ip: &IpAddr, ttl: u32) -> Result<()>;
    async fn update_dns_record(&self, record: &Self::DNSRecord, ip: &IpAddr) -> Result<()>;
    async fn delete_dns_record(&self, record: &Self::DNSRecord) -> Result<()>;
}

#[derive(Debug, Clone)]
struct HashSetItem<'a, T: Provider> {
    ip: &'a IpAddr,
    ref_record: Option<&'a T::DNSRecord>,
}

impl<'a, T: Provider> Hash for HashSetItem<'a, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ip.hash(state)
    }
}

impl<'a, T: Provider> PartialOrd for HashSetItem<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.ip.partial_cmp(other.ip)
    }
}

impl<'a, T: Provider> PartialEq<Self> for HashSetItem<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.ip.eq(other.ip)
    }
}

impl<'a, T: Provider> Eq for HashSetItem<'a, T> {}

#[async_trait]
pub(crate) trait DynProvider: Send + Sync {
    async fn check_and_update(&self, new_ips: &[IpAddr], ttl: u32, force: bool, family: IpType) -> Result<Vec<IpAddr>>;
}

#[async_trait]
impl<P> DynProvider for P
where
    P: Provider,
{
    async fn check_and_update(&self, new_ips: &[IpAddr], ttl: u32, force: bool, family: IpType) -> Result<Vec<IpAddr>> {
        let mut real_used_ips = vec![];
        let dns_records = self.get_dns_record(family).await?;
        if dns_records.is_empty() {
            info!("remote dns record(s) is empty");
        } else {
            let ips_str = dns_records.iter().map(|v| v.as_ref().to_string()).collect::<Vec<_>>().join(",");
            info!("got dns record(s) from remote: [{}]", ips_str);
        }
        let new_ip_set: HashSet<_> = new_ips.iter().map(|v| HashSetItem::<'_, P> { ip: v, ref_record: None }).collect();
        let dns_record_set: HashSet<_> =
            dns_records.iter().map(|v| HashSetItem::<'_, P> { ip: v.as_ref(), ref_record: Some(v) }).collect();
        let mut news: Vec<_> = new_ip_set.difference(&dns_record_set).collect();
        let mut olds: Vec<_> = dns_record_set.difference(&new_ip_set).collect();
        if force {
            let sames: Vec<_> = dns_record_set.intersection(&new_ip_set).collect();
            for item in sames {
                let record = item.ref_record.unwrap();
                let ip = item.ip;
                info!("force updating dns record to {}", ip);
                self.update_dns_record(record, ip).await?;
                real_used_ips.push(*ip);
            }
        }
        while let (Some(old_item), Some(new_item)) = (olds.get(0), news.get(0)) {
            let record = old_item.ref_record.unwrap();
            let new_ip = new_item.ip;
            olds.remove(0);
            news.remove(0);
            info!("updating dns record to {}", new_ip);
            self.update_dns_record(record, new_ip).await?;
            real_used_ips.push(*new_ip);
        }
        for old_item in olds {
            info!("target ip {} not belong to this interface, delete it", old_item.ip);
            self.delete_dns_record(old_item.ref_record.unwrap()).await?;
        }
        for new_item in news {
            info!("target ip {} not exist in dns provider, create it", new_item.ip);
            self.create_dns_record(new_item.ip, ttl).await?;
            real_used_ips.push(*new_item.ip);
        }
        if real_used_ips.is_empty() {
            info!("remote and local are the same nothing to do");
        }
        Ok(real_used_ips)
    }
}

#[inline]
pub(crate) fn record_type_from_ip(ip: &IpAddr) -> &'static str {
    match ip {
        IpAddr::V4(_) => "A",
        IpAddr::V6(_) => "AAAA",
    }
}
