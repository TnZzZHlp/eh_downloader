use std::{
    sync::{Arc, LazyLock, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar};
use reqwest::{Client, Proxy};

use crate::config::Config;

mod config;
mod gallery;
mod utils;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static SEM: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
static PB: LazyLock<MultiProgress> = LazyLock::new(MultiProgress::new);

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value = "./config.json")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let config = Arc::new(config::Config::read_from_file(&args.config).map_err(|e| {
        eprintln!("Error reading config file: {}", e);
        std::process::exit(1);
    })?);

    init(&config).map_err(|e| {
        eprintln!("Error initializing HTTP client: {}", e);
        std::process::exit(1);
    })?;

    let gallerys = config.get_links().map_err(|e| {
        eprintln!("Error reading input file: {}", e);
        std::process::exit(1);
    })?;

    let pb = PB.add(ProgressBar::new(gallerys.len() as u64));
    pb.enable_steady_tick(Duration::from_millis(100));
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{wide_bar:.cyan/blue}] [{pos}/{len}] [{msg}] ")
            .unwrap()
            .progress_chars("=>-"),
    );

    for g in gallerys {
        let config = Arc::clone(&config);
        g.download(config).await;
        pb.inc(1);
    }

    pb.finish_with_message("All downloads completed");

    Ok(())
}

fn init(config: &Config) -> Result<()> {
    // Initialize CLIENT
    let mut builder = Client::builder().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36 Edg/137.0.0.0").redirect(reqwest::redirect::Policy::none());

    if let Some(proxy_str) = &config.proxy {
        let proxy = Proxy::all(proxy_str).context("Invalid proxy URL")?;
        builder = builder.proxy(proxy);
    }

    let client = builder.build().context("Failed to build HTTP client")?;
    CLIENT
        .set(client)
        .expect("Failed to set the reqwest client");

    // Initialize SEM
    let semaphore = tokio::sync::Semaphore::new(config.concurrency as usize);
    SEM.set(Arc::new(semaphore))
        .expect("Failed to set the semaphore for concurrency control");

    Ok(())
}
