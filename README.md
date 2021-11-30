# ddns-rs

[![Rust](https://img.shields.io/badge/rust-nightly-brightgreen.svg)](https://www.rust-lang.org)
[![CI Status](https://github.com/ddns-rs/ddns-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ddns-rs/ddns-rs/actions/workflows/ci.yml)
[![Crate Status](https://img.shields.io/crates/v/ddns-rs.svg)](https://crates.io/crates/ddns-rs)

`ddns-rs` is an easy to used program that help you update your dns record.

## Features

* Both support **IPV4** and **IPV6**
* Support many DNS providers
* Support auto create dns record to provider
* Support many ways to obtain IP
* Multitasking support

## Quick start

Crate your config file `config.toml` like this:

``` toml
[base]
task_startup_interval = 5
task_retry_timeout = 10

[tasks]
t1 = {provider = "p1", family = "ipv4", interval = 10, interface = "i1", notifiers = ["n1"]}

[providers]
p1 = {kind = "cloudflare", force = false, ttl = 600, token = "your_cloudflare_token", dns = "www.example.com"}

[interfaces]
i1 = {kind = "stock", name = "eth0"}

[notifiers]
n1 = {kind = "empty"}
```

### Run in background:
```shell
.\ddns-rs -vvv -d
```

### Run as systemd service:

Create account for `ddns-rs` running

```shell
sudo adduser --system --gecos "DDNS-RS Service" --disabled-password --group --no-create-home ddns
```

Move the `ddns-rs` to `/usr/bin` and then chown

```shell
sudo chown ddns:ddns /usr/bin/ddns-rs
```

Create the directory that is required by ddns-rs

```shell
sudo mkdir /var/log/ddns-rs
sudo chown -R ddns:ddns /var/log/ddns-rs
```

Move your `config.toml` to `/etc/ddns-rs`

```shell
sudo mkdir /etc/ddns-rs
sudo chown -R ddns:ddns /etc/ddns-rs
```

Create systemd service:
```shell
sudo tee /etc/systemd/system/ddns-rs.service > /dev/null <<EOF
[Unit]
Description=ddns-rs service
Wants=network-online.target
After=local-fs.target network-online.target nss-lookup.target

[Service]
Type=exec
User=ddns
Group=ddns
UMask=007
PrivateTmp=false
ExecStart=/usr/bin/ddns-rs -vvv -C /etc/ddns-rs -L /var/log/ddns-rs
TimeoutStopSec=60
Restart=on-failure
SyslogIdentifier=ddns-rs

[Install]
WantedBy=multi-user.target
EOF
```

Start `ddns-rs`

```shell
sudo systemctl daemon-reload
sudo systemctl start ddns-rs.service
```

Enable `ddns-rs`

```shell
sudo systemctl enable ddns-rs.service
```

## Document

### Base

```toml
[base]
task_startup_interval = 10
task_retry_timeout = 10
```

The `task_startup_interval` field specific task start interval.

The `task_retry_timeout` field specific task retry timeout when task failed.


### Provider

The `ttl` field is supported by all interfaces, used when auto create dns record.

The `force` field is supported by all interfaces, meaning that the record is forced to be updated 
even if the target IP address is already the value we want to update.

The `kind` field indicates which provider will be used.

Currently, we support the following providers

* [Cloudflare](#Cloudflare)
* [Godaddy](#Godaddy)
* [Fake](#Fake)

#### Cloudflare

```toml
kind = "cloudflare"
force = false
ttl = 600
token = "your_cloudflare_token"
```

#### Godaddy

```toml
kind = "godaddy"
force = false
ttl = 600
api_key = "your_cloudflare_api_key"
secret = "your_cloudflare_secret"
```

#### Fake

```toml
force = false
ttl = 600
kind = "fake"
```

A placeholder provider, usually used with a notifier. So the meaning of the `force` field has a little difference, 
when `force` is `true`, notification are sent even if the current value is the same as the previous value that cached by 
the fake provider.

The `ttl` here has a different meaning, it indicates how long it will take for the record that stored in 
the `Fake Provider` to be deleted

### Interface

Currently, we support the following interfaces

* [Stock](#Stock), meaning get the IP from interface self
* [Peer](#peer), meaning get the IP from the server you specify

#### Stock

```toml
kind = "stock"
name = "you_interface_name"
```

#### Peer

```toml
kind = "peer"
url_v4 = "url_of_return_ipv4_address"
url_v6 = "url_of_return_ipv6_address"
ipv4_field_path = "regex:<capture_group_number:expression>"
ipv6_field_path = "json:</path_of_ip_field>"
```

### Notifier

Currently, we support the following notifiers

* [Empty](#Empty)
* [Email](#Email)
* [Webhook](#Webhook)

#### Empty

A placeholder notifier, nothing to do.

```toml
kind = "empty"
```

#### Email

Send an email when ip address has been changed.

```toml
kind = "email"
smtp_host = ""
smtp_port = ""
smtp_starttls = true
smtp_username = ""
smtp_password = ""
subject = ""
from = ""
to = ""
```

The `from` is optional, default is same as smtp_username.

The `subject` is optional, default is `DDNS-RS Notification`。

#### Webhook

Call your webhook when ip address has been changed.

```toml
kind = "webhook"
url = ""
authorization_header = ""
local_address = ""
```

The `local_address` can be `0.0.0.0` or `::` to force the ip family to be used。

### Task

```toml
provider = "name_of_provider_in_the_config_file"
family = "ipv4" # ipv4, ipv6, all
interval = 10 # in second
autostart = true # default true
interface = "name_of_interface_in_the_config_file"
notifiers = ["name_of_notifier_in_the_config_file"]
```

## License

[MIT](LICENSE)
