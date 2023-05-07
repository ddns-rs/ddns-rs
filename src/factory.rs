use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use toml::Value;

use crate::interfaces::Interface;
use crate::notifiers::Notifier;
use crate::providers::DynProvider;
use crate::{interfaces, notifiers, providers, Shutdown};

macro_rules! from_args_str {
    ($args:ident, $key:literal) => {{
        let _hidden = $args.get($key).ok_or(anyhow!(concat!("missing ", $key, " arg")))?;
        _hidden
            .as_str()
            .ok_or(anyhow!(concat!("arg ", $key, " unknown type")))?
    }};
}

macro_rules! option_from_args_str {
    ($args:ident, $key:literal) => {{
        if let Some(_hidden) = $args.get($key) {
            Some(
                _hidden
                    .as_str()
                    .ok_or(anyhow!(concat!("arg ", $key, " unknown type")))?,
            )
        } else {
            None
        }
    }};
}

macro_rules! option_from_args_bool {
    ($args:ident, $key:literal) => {{
        if let Some(_hidden) = $args.get($key) {
            Some(
                _hidden
                    .as_bool()
                    .ok_or(anyhow!(concat!("arg ", $key, " unknown type")))?,
            )
        } else {
            None
        }
    }};
}

macro_rules! option_from_args_integer {
    ($args:ident, $key:literal) => {{
        if let Some(_hidden) = $args.get($key) {
            Some(
                _hidden
                    .as_integer()
                    .ok_or(anyhow!(concat!("arg ", $key, " unknown type")))?,
            )
        } else {
            None
        }
    }};
}

pub(crate) async fn create_interface<S: AsRef<str>>(
    kind: S,
    args: HashMap<String, Value>,
) -> Result<Box<dyn Interface>> {
    let interface: Box<dyn Interface> = match kind.as_ref() {
        "peer" => {
            let url_v4 = from_args_str!(args, "url_v4");
            let url_v6 = from_args_str!(args, "url_v6");
            let ipv4_field_path = from_args_str!(args, "ipv4_field_path");
            let ipv6_field_path = from_args_str!(args, "ipv6_field_path");
            Box::new(interfaces::Peer::create(
                url_v4,
                url_v6,
                ipv4_field_path,
                ipv6_field_path,
            )?)
        },
        "stock" => {
            let name = from_args_str!(args, "name");
            Box::new(interfaces::Stock::create(name)?)
        },
        _ => {
            bail!("the kind of interface '{}' not support", kind.as_ref())
        },
    };
    Ok(interface)
}

pub(crate) async fn create_notifier<S: AsRef<str>>(
    kind: S,
    args: HashMap<String, Value>,
) -> Result<Option<Box<dyn Notifier>>> {
    let notifier: Option<Box<dyn Notifier>> = match kind.as_ref() {
        "email" => {
            let smtp_username = from_args_str!(args, "smtp_username");
            let smtp_password = from_args_str!(args, "smtp_password");
            let smtp_host = from_args_str!(args, "smtp_host");
            let smtp_port = option_from_args_integer!(args, "smtp_port");
            let smtp_starttls = option_from_args_bool!(args, "smtp_starttls").unwrap_or(true);
            let to = from_args_str!(args, "to");
            let subject = option_from_args_str!(args, "subject");
            let from = option_from_args_str!(args, "from");
            Some(Box::new(
                notifiers::Email::create(
                    smtp_username,
                    smtp_password,
                    smtp_host,
                    smtp_port.map(|v| v as u16),
                    smtp_starttls,
                    subject,
                    from,
                    to,
                )
                .await?,
            ))
        },
        "webhook" => {
            let url = from_args_str!(args, "url");
            let authorization_header = from_args_str!(args, "authorization_header");
            let local_address = option_from_args_str!(args, "local_address");
            let local_address = if let Some(local_address) = local_address {
                Some(local_address.parse::<IpAddr>()?)
            } else {
                None
            };
            Some(Box::new(
                notifiers::Webhook::create(url, authorization_header, local_address).await?,
            ))
        },
        "empty" => None,
        _ => {
            bail!("the kind of notifiers '{}' not support", kind.as_ref())
        },
    };
    Ok(notifier)
}

pub(crate) async fn create_provider<S: AsRef<str>>(
    shutdown: Arc<Shutdown>,
    kind: S,
    args: HashMap<String, Value>,
) -> Result<Box<dyn DynProvider>> {
    let provider: Box<dyn DynProvider> = match kind.as_ref() {
        "cloudflare" => {
            let token = from_args_str!(args, "token");
            let dns = from_args_str!(args, "dns");
            Box::new(providers::Cloudflare::create(token, dns).await?)
        },
        "godaddy" => {
            let api_key = from_args_str!(args, "api_key");
            let secret = from_args_str!(args, "secret");
            let dns = from_args_str!(args, "dns");
            Box::new(providers::Godaddy::create(api_key, secret, dns).await?)
        },
        "fake" => Box::new(providers::Fake::create(shutdown).await?),
        _ => {
            bail!("the kind of provider '{}' not support", kind.as_ref())
        },
    };
    Ok(provider)
}
