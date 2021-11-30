use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
use lettre::message::{header, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use maud::html;

use crate::notifiers::Notifier;

pub struct Email {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    subject: String,
    to: String,
    from: String,
}

impl Email {
    #[allow(clippy::too_many_arguments)]
    pub async fn create<S: AsRef<str>>(
        smtp_username: S,
        smtp_password: S,
        smtp_host: S,
        smtp_port: Option<u16>,
        smtp_starttls: bool,
        subject: Option<S>,
        from: Option<S>,
        to: S,
    ) -> Result<Email> {
        let smtp_username = smtp_username.as_ref().to_owned();
        let smtp_password = smtp_password.as_ref().to_owned();
        let smtp_host = smtp_host.as_ref().to_owned();
        let mut mailer_builder = if smtp_starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_host)?
        };
        if let Some(smtp_port) = smtp_port {
            mailer_builder = mailer_builder.port(smtp_port)
        }
        mailer_builder = mailer_builder
            .authentication(vec![Mechanism::Plain, Mechanism::Login, Mechanism::Xoauth2])
            .credentials(Credentials::from((&smtp_username, &smtp_password)));
        let mailer = mailer_builder.build();
        let subject = match subject {
            None => "DDNS-RS Notification".to_owned(),
            Some(subject) => subject.as_ref().to_owned(),
        };
        let from = match from {
            None => {
                format!("ddns-rs <{}>", &smtp_username)
            },
            Some(from) => from.as_ref().to_owned(),
        };
        let to = to.as_ref().to_owned();
        Ok(Email { mailer, subject, from, to })
    }
}

fn build_email(new_ips: &[IpAddr]) -> String {
    // Create the html we want to send.
    let html = html! {
        head {
            title { "DDNS-RS Notification" }
            style type="text/css" {
                "body {
                    margin: 0;
                    background: #fff;
                    display: grid;
                    place-items: center;
                    font-family: 'Source Sans Pro', sans-serif;
                }

                .box {
                    background: #F3F6F65E;
                    min-width: 480px;
                    max-width: 480px;
                    width: 100%;
                    border-radius: 12px;
                    box-shadow: 0 0 40px -10px rgba(0, 0, 0, .4);
                    display: block;
                }

                .header {
                    height: 50px;
                    background: #2a3439;
                    color: #fff;
                    border-radius: 12px 12px 0 0;
                    overflow: hidden;
                }

                .title {
                    display: block;
                    text-decoration: none;
                }

                .title-text {
                    padding: 10px 0 0 0;
                    display: block;
                    color: #fff;
                    text-align: center;
                    font-size: 25px;
                    font-weight: 700;
                    letter-spacing: 7px;
                }

                ol li {
                    list-style-type: none;
                    counter-increment: item;
                }

                ol li:before {
                    content: counter(item);
                    margin-right: 5px;
                    font-size: 80%;
                    background: #ffc832;
                    color: #2a3439;
                    font-weight: bold;
                    padding: 5px 10px;
                    border-radius: 3px;
                }

                .ip-box {
                    margin: 20px;
                    padding: 0;
                    display: block;
                }

                .ip-item {
                    color: #2a3439;
                    font-size: 19px;
                    font-weight: 500;
                    letter-spacing: 1px;
                    padding: 10px;
                }"
            }
        }
        div class="box" {
            div class="header" {
                a class="title" href="https://github.com/ddns-rs/ddns-rs" {
                    span class="title-text" { "DDNS-RS" }
                }
            }
            ol class="ip-box" {
                @for ip in new_ips.iter() {
                    li class="ip-item" {
                        (ip)
                    }
                }
            }
        }
    };
    html.into_string()
}

fn build_email_plaintext(new_ips: &[IpAddr]) -> String {
    let new_ips_str = new_ips.iter().map(|v| format!("\t{}\n", v)).collect::<Vec<_>>().concat();
    let logo = r#"
┌┬┐┌┬┐┌┐┌┌─┐   ┬─┐┌─┐
 ││ │││││└─┐───├┬┘└─┐
─┴┘─┴┘┘└┘└─┘   ┴└─└─┘
DNS record updater
"#;
    format!("{}New IP List:\n{}", logo, new_ips_str)
}

#[async_trait]
impl Notifier for Email {
    async fn send(&self, new_ips: &[IpAddr]) -> Result<()> {
        let email = Message::builder()
            .from(self.from.parse().unwrap())
            .to(self.to.parse().unwrap())
            .subject(&self.subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_PLAIN)
                            .body(build_email_plaintext(new_ips)),
                    )
                    .singlepart(
                        SinglePart::builder().header(header::ContentType::TEXT_HTML).body(build_email(new_ips)),
                    ),
            )
            .unwrap();

        self.mailer.send(email).await?;
        Ok(())
    }
}
