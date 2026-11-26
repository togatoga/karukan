//! karukan-ibus: the Karukan engine for IBus.
//!
//! Connects to ibus-daemon's bus and serves the IBus factory there, one
//! karukan engine per `CreateEngine`, the way libibus engines work.
//!
//! - `--ibus`: started by ibus-daemon from the installed component file;
//!   owns the component's bus name.
//! - no flag: started by hand; registers the component itself, so the
//!   engine is listed (`ibus engine karukan`) while this process runs.
//! - `--xml`: prints the component file (`karukan.xml`).
//! - `--version`: prints the build.
//!
//! Learning caches are saved when an engine is destroyed, on focus out,
//! when the daemon goes away, and on SIGTERM / SIGINT.

mod address;
mod ibus;
mod service;
mod session;
mod worker;

use anyhow::Context;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use ibus::V;
use service::{FACTORY_PATH, Factory};
use worker::Worker;

fn main() -> anyhow::Result<()> {
    let has = |flag: &str| std::env::args().skip(1).any(|a| a == flag);
    if has("--version") {
        println!("karukan-ibus {}", karukan_engine::VERSION);
        return Ok(());
    }
    if has("--xml") {
        print!(
            "{}",
            ibus::component(ibus::INSTALLED_EXEC.to_string()).to_xml()
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    zbus::block_on(run(has("--ibus")))
}

async fn run(started_by_daemon: bool) -> anyhow::Result<()> {
    let address = address::bus_address()?;
    let worker = Worker::spawn();
    let connection = zbus::connection::Builder::address(address.as_str())
        .serve_at(FACTORY_PATH, Factory::new(worker.clone()))
        .build()
        .await
        .with_context(|| format!("connect to ibus-daemon at {address}"))?;

    if started_by_daemon {
        connection.request_name(ibus::COMPONENT_NAME).await?;
    } else {
        let exec = format!("{} --ibus", std::env::current_exe()?.display());
        connection
            .call_method(
                Some("org.freedesktop.IBus"),
                "/org/freedesktop/IBus",
                Some("org.freedesktop.IBus"),
                "RegisterComponent",
                &(V(ibus::component(exec)),),
            )
            .await
            .context("RegisterComponent")?;
    }
    tracing::info!(
        "karukan-ibus {} ready (pid={})",
        karukan_engine::VERSION,
        std::process::id()
    );

    let mut signals = Signals::new([SIGTERM, SIGINT])?;
    let on_signal = worker.clone();
    std::thread::spawn(move || {
        if let Some(signal) = signals.forever().next() {
            tracing::info!("signal {signal}: saving learning caches");
            zbus::block_on(on_signal.save_all());
            std::process::exit(0);
        }
    });

    connection.closed().await;
    tracing::info!("ibus-daemon disconnected: saving learning caches");
    worker.save_all().await;
    Ok(())
}
