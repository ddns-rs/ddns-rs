use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use toml::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub family: String,
    pub provider: String,
    #[serde(default = "default_interval")]
    pub interval: u32,
    pub interface: String,
    pub notifiers: Vec<String>,
}

fn default_interval() -> u32 {
    60
}

impl Default for Task {
    fn default() -> Self {
        Task {
            interval: default_interval(),
            interface: Default::default(),
            family: Default::default(),
            provider: Default::default(),
            notifiers: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Provider {
    pub kind: String,
    pub force: bool,
    pub ttl: u32,
    #[serde(flatten)]
    pub args: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Interface {
    pub kind: String,
    #[serde(flatten)]
    pub args: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Notifier {
    pub kind: String,
    #[serde(flatten)]
    pub args: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Base {
    #[serde(default = "default_task_startup_interval")]
    pub task_startup_interval: u64,
    #[serde(default = "default_task_retry_timeout")]
    pub task_retry_timeout: u64,
}

fn default_task_startup_interval() -> u64 {
    5
}

fn default_task_retry_timeout() -> u64 {
    10
}

impl Default for Base {
    fn default() -> Self {
        Self {
            task_startup_interval: default_task_startup_interval(),
            task_retry_timeout: default_task_retry_timeout(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Setting {
    #[serde(default)]
    pub base: Base,
    pub tasks: HashMap<String, Task>,
    pub providers: HashMap<String, Provider>,
    pub interfaces: HashMap<String, Interface>,
    pub notifiers: HashMap<String, Notifier>,
}
