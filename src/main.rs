#![feature(ip)]
#![feature(async_closure)]

#[macro_use]
extern crate serde_json;

use std::collections::HashMap;
use std::env::{current_dir, set_current_dir};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use factory::{create_interface, create_notifier, create_provider};
use futures::prelude::*;
use interfaces::Interface;
use log::{debug, error, info, warn, LevelFilter};
use log4rs::append::console::{ConsoleAppender, Target};
use log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
use log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;
use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
use log4rs::append::rolling_file::RollingFileAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::filter::threshold::ThresholdFilter;
use notifiers::Notifier;
use providers::DynProvider;
use rand::prelude::*;
use setting::Setting;
use shutdown::Shutdown;
use tokio::time::{interval_at, sleep, Duration, Instant};
use tokio::{fs, join, pin, select, signal};

mod factory;
mod interfaces;
mod notifiers;
mod providers;
mod setting;
mod shutdown;
mod updater;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum IpType {
    V4,
    V6,
}

impl Display for IpType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            IpType::V4 => "IPV4",
            IpType::V6 => "IPV6",
        };
        write!(f, "{}", str)
    }
}

fn setup_logger(level: log::LevelFilter, log_direction: PathBuf) -> Result<log4rs::Handle> {
    let console_pattern = PatternEncoder::new("{h({d(%Y-%m-%d %H:%M:%S %Z)(local)} - {l} - {m})}\n");
    let file_pattern = PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(local)} - {l} - {m}\n");
    let file_path = log_direction.join("output.log");

    // Build a stderr logger.
    let console = ConsoleAppender::builder().encoder(Box::new(console_pattern)).target(Target::Stdout).build();

    // Logging to log file.
    let size_trigger = SizeTrigger::new(1024 * 1024);
    let rolling_file_pattern = format!(
        "{}/archive/output.{{}}.log",
        log_direction.as_os_str().to_str().ok_or_else(|| anyhow!("can't convert log direction to &str"))?
    );
    let rolling = FixedWindowRoller::builder().base(0).build(&rolling_file_pattern, 10)?;
    let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(rolling));
    let logfile = RollingFileAppender::builder().encoder(Box::new(file_pattern)).build(file_path, Box::new(policy))?;

    let config = log4rs::Config::builder()
        .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(level))).build("logfile", Box::new(logfile)))
        .appender(Appender::builder().filter(Box::new(ThresholdFilter::new(level))).build("console", Box::new(console)))
        .build(Root::builder().appender("logfile").appender("console").build(LevelFilter::Trace))?;
    Ok(log4rs::init_config(config)?)
}

async fn run_task(
    families: &[IpType],
    provider: (Arc<Box<dyn DynProvider>>, u32, bool),
    interface: Arc<Box<dyn Interface>>,
    notifiers: Vec<Arc<Option<Box<dyn Notifier>>>>,
) -> Result<()> {
    let (provider, ttl, force) = provider;
    for family in families {
        let target_ips = interface.get_ip(*family).await?;
        let ips_str = target_ips.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        // check if the IP is legal
        if target_ips.iter().any(|ip| match family {
            IpType::V4 => ip.is_ipv6(),
            IpType::V6 => ip.is_ipv4(),
        }) {
            warn!("ip(s) from interface is illegal, require {} but got: [{}]", family, ips_str);
            continue;
        }
        info!("got ip(s) from interface: [{}]", ips_str);
        let update_ips = provider.check_and_update(&target_ips, ttl, force, *family).await?;
        if !update_ips.is_empty() {
            for notifier in notifiers.clone() {
                if let Some(notifier) = &*notifier {
                    notifier.send(&update_ips).await?;
                }
            }
        }
    }
    Ok(())
}

async fn run(shutdown: Arc<Shutdown>, setting: Setting) -> Result<()> {
    let base = setting.base;
    debug!("building interfaces");
    let mut interface_map = HashMap::new();
    for (name, interface) in setting.interfaces {
        let interface = create_interface(interface.kind, interface.args).await?;
        interface_map.insert(name, Arc::new(interface));
    }

    debug!("building notifiers");
    let mut notifier_map = HashMap::new();
    for (name, notifier) in setting.notifiers {
        let notifier = create_notifier(notifier.kind, notifier.args).await?;
        notifier_map.insert(name, Arc::new(notifier));
    }

    debug!("building providers");
    let mut provider_map = HashMap::new();
    for (name, provider) in setting.providers {
        let force = provider.force;
        let ttl = provider.ttl;
        let provider = create_provider(shutdown.clone(), provider.kind, provider.args).await?;
        provider_map.insert(name, (Arc::new(provider), ttl, force));
    }

    let shutdown_for_create_all_task = shutdown.clone();
    let create_task = move |start_delay: Duration, task: &setting::Task| -> Result<_> {
        let shutdown_for_create_all_task = shutdown_for_create_all_task.clone();
        let family = &*task.family;
        let families: &[IpType] = match family {
            "ipv4" => &[IpType::V4],
            "ipv6" => &[IpType::V6],
            "all" => &[IpType::V4, IpType::V6],
            _ => {
                bail!("unknown family {}", family)
            },
        };
        let mut notifiers = vec![];
        for notifier in &task.notifiers {
            let notifier = notifier_map.get(&*notifier).ok_or_else(|| anyhow!("can't find notifier define"))?.clone();
            notifiers.push(notifier);
        }
        let interface =
            interface_map.get(&*task.interface).ok_or_else(|| anyhow!("can't find interface define"))?.clone();
        let provider = provider_map.get(&*task.provider).ok_or_else(|| anyhow!("can't find provider define"))?.clone();
        let interval_duration = Duration::from_secs(task.interval as u64);
        Ok(Box::pin(async move {
            let start = Instant::now() + start_delay;
            let mut check_timer = interval_at(start, interval_duration);
            loop {
                select! {
                    _ = shutdown_for_create_all_task.receive() => {
                        break;
                    },
                    _ = check_timer.tick() => {}
                }
                run_task(families, provider.clone(), interface.clone(), notifiers.clone()).await?;
            }

            #[allow(unreachable_code)]
            Result::<_>::Ok(())
        }))
    };

    debug!("building task");
    let shutdown_for_shutdown_task = shutdown.clone();
    let shutdown_task = Box::pin(async move {
        shutdown_for_shutdown_task.receive().await;
        Ok(())
    });
    let mut task_handles: Vec<Pin<Box<dyn Future<Output = Result<()>>>>> = vec![shutdown_task];
    let mut tasks = vec![];
    for (i, (_, task)) in setting.tasks.iter().enumerate() {
        let future = create_task(Duration::from_secs(base.task_startup_interval * i as u64), task)?;
        task_handles.push(future);
        tasks.push(task);
    }

    debug!("starting tasks");
    let mut rng = thread_rng();
    loop {
        if task_handles.is_empty() {
            info!("no alive task");
            return Ok(());
        }
        let (result, task_index, mut remain) = future::select_all(task_handles).await;
        if task_index == 0 {
            info!("waiting remain task complete");
            future::join_all(remain).await;
            return Ok(());
        }
        result.unwrap_or_else(|err| error!("task happen error: {}", err));
        let future = create_task(
            Duration::from_secs(base.task_retry_timeout + rng.gen_range(0..5)),
            tasks.get(task_index - 1).unwrap(),
        )?;
        remain.insert(task_index, future);
        task_handles = remain;
    }
}

/// ┌┬┐┌┬┐┌┐┌┌─┐   ┬─┐┌─┐
///  ││ │││││└─┐───├┬┘└─┐
/// ─┴┘─┴┘┘└┘└─┘   ┴└─└─┘
/// DNS record updater
#[derive(Parser, Debug)]
#[clap(name = "ddns-rs", version = "1.0", author = "Honsun Zhu <honsun@linux.com>", verbatim_doc_comment)]
struct Opts {
    /// Path of config file
    #[clap(short, long, default_value = "config.toml")]
    config: String,
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
    /// Nothing to output if specified
    #[clap(short, long)]
    silence: bool,
    /// Run as a daemon
    #[cfg(target_family = "unix")]
    #[clap(short, long)]
    daemon: bool,
    /// Used with --daemon, the path of the pid
    #[clap(short, long)]
    pid_path: Option<String>,
    /// Current direction, it will use '.' if not specified
    #[clap(short = 'C', long, parse(from_os_str))]
    current_direction: Option<PathBuf>,
    /// Current direction, it will use '.' if not specified
    #[clap(short = 'L', long, parse(from_os_str))]
    log_direction: Option<PathBuf>,
}

#[tokio::main]
async fn real_main(config_file: String, log_level: log::LevelFilter, log_direction: PathBuf) {
    // setup logger
    setup_logger(log_level, log_direction).expect("can't setup logger");

    let shutdown = Arc::new(Shutdown::new());
    let mut retry = false;
    'outer: loop {
        // loading config
        debug!("reading config from: {}", &config_file);
        let setting_contents = match fs::read_to_string(&config_file).await {
            Ok(v) => v,
            Err(err) => {
                error!("can't read config: {}", err);
                return;
            },
        };
        let setting: Setting = match toml::from_str(&setting_contents) {
            Ok(v) => v,
            Err(err) => {
                error!("can't parse config: {}", err);
                return;
            },
        };
        info!("started");
        #[cfg(target_os = "linux")]
        {
            use sd_notify::NotifyState;
            let _ = sd_notify::notify(true, &[NotifyState::Reloading]);
            let _ = sd_notify::notify(true, &[NotifyState::Ready]);
        }

        loop {
            // prepare main logic
            let run_task = run(shutdown.clone(), setting.clone());
            pin!(run_task);

            let reload_sig = async move {
                #[cfg(target_os = "linux")]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sighup = signal(SignalKind::hangup())?;
                    sighup.recv().await;
                    return Ok(());
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let () = future::pending().await;
                    unreachable!();
                }
                #[allow(unreachable_code)]
                Result::<_>::Ok(())
            };
            pin!(reload_sig);

            let exit_sig = async move {
                #[cfg(target_os = "linux")]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut sigterm = signal(SignalKind::terminate())?;
                    select! {
                        _ = sigterm.recv() => {},
                        v = signal::ctrl_c() => v?,
                    };
                    return Ok(());
                }
                #[cfg(not(target_os = "linux"))]
                {
                    signal::ctrl_c().await?;
                    return Ok(());
                }
                #[allow(unreachable_code)]
                Result::<_>::Ok(())
            };
            pin!(exit_sig);

            select! {
                result = exit_sig => {
                    match result {
                        Ok(()) => {
                            info!("receive signal interrupt -> exec graceful shutdown");
                            let (result, _) = join!(run_task, shutdown.shutdown());
                            if let Err(err) = result {
                                error!("unexpected error: {}", err);
                            }
                            info!("shutdown");
                            break 'outer;
                        },
                        Err(err) => {
                            error!("unable to listen for shutdown signal: {}", err);
                            return;
                        },
                    }
                },
                result = reload_sig => {
                    match result {
                        Ok(()) => {
                            info!("receive reload signal -> reload setting");
                            // waiting for unfinished tasks to finish before the signal
                            // and then reload the new setting
                            let (result, _) = join!(run_task, shutdown.shutdown());
                            if let Err(err) = result {
                                error!("unexpected error: {}", err);
                            }
                            break;
                        },
                        Err(err) => {
                            error!("unable to listen for reload signal: {}", err);
                            return;
                        },
                    }
                },
                result = &mut run_task, if !retry => {
                    if let Err(err) = result {
                        error!("unexpected error: {}", err);
                        retry = true
                    }
                },
                _ = sleep(Duration::from_secs(10)), if retry => {
                    retry = false
                }
            }
        }
    }
}

fn main() {
    // parse from cmdline
    let opts: Opts = Opts::parse();

    let log_level = if opts.silence {
        log::LevelFilter::Off
    } else {
        match opts.verbose {
            0 | 1 => log::LevelFilter::Error,
            2 => log::LevelFilter::Warn,
            3 => log::LevelFilter::Info,
            4 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        }
    };

    let current_direction = if let Some(v) = opts.current_direction {
        set_current_dir(&v).unwrap_or_else(|err| warn!("can't change current direction: {}", err));
        v
    } else {
        match current_dir() {
            Ok(dir) => dir,
            Err(err) => {
                warn!("can't get current direction: {}", err);
                return;
            },
        }
    };

    let log_direction = opts.log_direction.unwrap_or_else(|| current_direction.clone());

    #[cfg(target_family = "unix")]
    {
        use daemonize::Daemonize;

        if opts.daemon {
            info!("starting as a daemon");
            let mut daemonize = Daemonize::new();
            if let Some(pid_file) = opts.pid_path {
                daemonize = daemonize.pid_file(pid_file).chown_pid_file(true);
            }
            if log_level > log::LevelFilter::Info {
                let stdout = File::create("daemon-dbg.out").expect("can't create daemon stdout file");
                let stderr = File::create("daemon-dbg.err").expect("can't create daemon stderr file");

                daemonize = daemonize.stdout(stdout).stderr(stderr)
            }
            daemonize = daemonize.working_directory(current_direction).exit_action(|| {});
            match daemonize.start() {
                Ok(_) => {
                    real_main(opts.config, log_level, log_direction);
                },
                Err(err) => {
                    error!("can't start daemonize: {}", err);
                },
            }
        } else {
            info!("starting");
            real_main(opts.config, log_level, log_direction);
        }
    }

    #[cfg(not(target_family = "unix"))]
    {
        info!("starting");
        real_main(opts.config, log_level, log_direction);
    }
}
