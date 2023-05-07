use std::collections::{BinaryHeap, HashMap};
use std::fmt::{Display, Formatter};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep_until, Duration, Instant};

use crate::providers::Provider;
use crate::{IpType, Shutdown};

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct DNSRecord {
    pub id: u32,
    pub ip: IpAddr,
    pub ttl: u32,
    pub deadline: Instant,
}

impl PartialOrd for DNSRecord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.deadline.partial_cmp(&other.deadline)
    }
}

impl Ord for DNSRecord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.deadline.cmp(&other.deadline)
    }
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

pub struct Fake {
    id_index: AtomicU32,
    ipv4_cache: Arc<Mutex<HashMap<u32, DNSRecord>>>,
    ipv6_cache: Arc<Mutex<HashMap<u32, DNSRecord>>>,
    tx: Sender<DNSRecord>,
}

impl Fake {
    pub async fn create(shutdown: Arc<Shutdown>) -> Result<Self> {
        let ipv4_cache = Arc::new(Mutex::new(HashMap::with_capacity(10)));
        let ipv6_cache = Arc::new(Mutex::new(HashMap::with_capacity(10)));
        let (tx, mut rx) = mpsc::channel::<DNSRecord>(10);
        let result = Fake {
            id_index: AtomicU32::new(1),
            ipv4_cache: ipv4_cache.clone(),
            ipv6_cache: ipv6_cache.clone(),
            tx,
        };
        tokio::spawn(async move {
            let mut ttl_heap = BinaryHeap::<DNSRecord>::new();
            loop {
                select! {
                    _ = shutdown.receive() => {
                        break;
                    },
                    record = rx.recv() => {
                        if let Some(record) = record {
                            ttl_heap.push(record);
                        } else {
                            break;
                        }
                    },
                    record = async {
                        let record = ttl_heap.pop().unwrap();
                        sleep_until(record.deadline).await;
                        match record.ip {
                            IpAddr::V4(_) => {
                                let mut ipv4_cache = ipv4_cache.lock().await;
                                ipv4_cache.remove(&record.id);
                            }
                            IpAddr::V6(_) => {
                                let mut ipv6_cache = ipv6_cache.lock().await;
                                ipv6_cache.remove(&record.id);
                            }
                        }
                        record
                    }, if !ttl_heap.is_empty() => {
                        info!("the TTL of the record has been exceeded, delete the record {}", record.id);
                    }
                }
            }
        });
        Ok(result)
    }
}

#[async_trait]
impl Provider for Fake {
    type DNSRecord = DNSRecord;

    async fn get_dns_record(&self, family: IpType) -> Result<Vec<Self::DNSRecord>> {
        match family {
            IpType::V4 => {
                let ipv4_cache = self.ipv4_cache.lock().await;
                Ok(ipv4_cache.iter().map(|(_, v)| v).cloned().collect())
            },
            IpType::V6 => {
                let ipv6_cache = self.ipv6_cache.lock().await;
                Ok(ipv6_cache.iter().map(|(_, v)| v).cloned().collect())
            },
        }
    }

    async fn create_dns_record(&self, ip: &IpAddr, ttl: u32) -> Result<()> {
        let id = self.id_index.fetch_add(1, Ordering::SeqCst);
        match ip {
            IpAddr::V4(_) => {
                let mut ipv4_cache = self.ipv4_cache.lock().await;
                let deadline = Instant::now() + Duration::from_secs(ttl as u64);
                let record = DNSRecord {
                    id,
                    ip: *ip,
                    ttl,
                    deadline,
                };
                ipv4_cache.insert(id, record.clone());
                self.tx.send(record).await?;
            },
            IpAddr::V6(_) => {
                let mut ipv6_cache = self.ipv6_cache.lock().await;
                let deadline = Instant::now() + Duration::from_secs(ttl as u64);
                let record = DNSRecord {
                    id,
                    ip: *ip,
                    ttl,
                    deadline,
                };
                ipv6_cache.insert(id, record.clone());
                self.tx.send(record).await?;
            },
        }
        Ok(())
    }

    async fn update_dns_record(&self, record: &Self::DNSRecord, ip: &IpAddr) -> Result<()> {
        let id = record.id;
        match ip {
            IpAddr::V4(_) => {
                let mut ipv4_cache = self.ipv4_cache.lock().await;
                let record = ipv4_cache.get_mut(&id).ok_or_else(|| anyhow!("can't find records"))?;
                record.ip = *ip;
            },
            IpAddr::V6(_) => {
                let mut ipv6_cache = self.ipv6_cache.lock().await;
                let record = ipv6_cache.get_mut(&id).ok_or_else(|| anyhow!("can't find records"))?;
                record.ip = *ip;
            },
        }
        Ok(())
    }

    async fn delete_dns_record(&self, record: &Self::DNSRecord) -> Result<()> {
        let id = record.id;
        match record.ip {
            IpAddr::V4(_) => {
                let mut ipv4_cache = self.ipv4_cache.lock().await;
                ipv4_cache.remove(&id).ok_or_else(|| anyhow!("can't find records"))?;
            },
            IpAddr::V6(_) => {
                let mut ipv6_cache = self.ipv6_cache.lock().await;
                ipv6_cache.remove(&id).ok_or_else(|| anyhow!("can't find records"))?;
            },
        }
        Ok(())
    }
}
