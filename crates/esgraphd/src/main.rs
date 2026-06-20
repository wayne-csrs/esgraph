//! # esgraphd
//!
//! Command-line interface for esgraph.
//!
//! ## Commands
//!
//! | Command | Purpose |
//! |---------|---------|
//! | `replay` | Ingest JSON fixtures into LadybugDB (no ESF / no VM) |
//! | `query` | Run Cypher hunt queries against the graph |
//! | `status` | Show node/relationship counts |
//! | `run` | Live ESF subscription → LadybugDB writer (macOS + entitlement) |
//!
//! ## Typical host-only workflow
//!
//! ```text
//! cargo build -p esgraphd
//! ./target/debug/esgraphd replay --config config/default.toml fixtures/*.json
//! ./target/debug/esgraphd status --config config/default.toml
//! ./target/debug/esgraphd query --config config/default.toml \
//!     "MATCH (p:Process)-[r:WROTE]->(f:File) RETURN p.path, f.path LIMIT 20"
//! ```

mod replay;
mod writer;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use esgraph_core::Config;
use esgraph_esf::{run_collector, EsfError};
use esgraph_store::{GraphStore, GraphStoreOptions};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

/// esgraph — ESF events → LadybugDB graph
#[derive(Parser, Debug)]
#[command(name = "esgraphd", version, about)]
struct Cli {
    /// Path to TOML configuration file.
    #[arg(long, short, default_value = "config/default.toml", global = true)]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Ingest JSON fixture files as normalised ESF events (no live ESF).
    Replay {
        /// One or more `.json` fixture files (object or array of events).
        fixtures: Vec<PathBuf>,
    },
    /// Execute a Cypher query and print tab-separated results.
    Query {
        /// Cypher statement (quote for the shell).
        cypher: String,
    },
    /// Print graph path and node/relationship counts.
    Status,
    /// Subscribe to live ESF events and ingest into LadybugDB.
    Run,
}

fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let config = Config::from_file(&cli.config)
        .with_context(|| format!("load config {}", cli.config.display()))?;

    let options = GraphStoreOptions::default();

    match cli.command {
        Commands::Replay { fixtures } => {
            if fixtures.is_empty() {
                bail!("replay requires at least one fixture path");
            }
            cmd_replay(&config, options, &fixtures)
        }
        Commands::Query { cypher } => cmd_query(&config, options, &cypher),
        Commands::Status => cmd_status(&config, options),
        Commands::Run => cmd_run(&config, options),
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("esgraph=info")),
        )
        .init();
}

fn open_store(config: &Config, options: GraphStoreOptions) -> Result<GraphStore> {
    GraphStore::from_config(&config.store, options).map_err(|e| anyhow::anyhow!("{e}"))
}

fn cmd_replay(config: &Config, options: GraphStoreOptions, fixtures: &[PathBuf]) -> Result<()> {
    let events = replay::load_fixtures(fixtures)?;
    info!(count = events.len(), "loaded fixtures");

    let store = open_store(config, options)?;
    let stats = store.ingest(&events).map_err(|e| anyhow::anyhow!("{e}"))?;

    println!(
        "ingested {} event(s), {} node upsert(s), {} edge(s)",
        stats.events, stats.node_upserts, stats.edges_inserted
    );
    println!("graph: {}", config.store.path);
    Ok(())
}

fn cmd_query(config: &Config, options: GraphStoreOptions, cypher: &str) -> Result<()> {
    let store = open_store(config, options)?;
    let result = store
        .query_tabular(cypher)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if result.columns.is_empty() {
        println!("(no columns)");
    } else {
        println!("{}", result.columns.join("\t"));
        for row in &result.rows {
            println!("{}", row.join("\t"));
        }
    }
    println!("({} rows)", result.rows.len());
    Ok(())
}

fn cmd_status(config: &Config, options: GraphStoreOptions) -> Result<()> {
    let store = open_store(config, options)?;

    println!("graph: {}", config.store.path);
    println!();

    for label in ["IngestEvent", "Process", "File", "Socket"] {
        let count = store
            .count_label(label)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!("{label}: {count}");
    }

    println!();
    for rel in [
        "EXECUTED",
        "FORKED",
        "EXITED",
        "WROTE",
        "CREATED",
        "UNLINKED",
        "RENAMED",
        "UIPC_BOUND",
        "UIPC_CONNECTED",
    ] {
        let count = store
            .count_relationship(rel)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!("{rel}: {count}");
    }

    let events = config
        .resolved_event_names()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!();
    println!("configured ESF subscriptions:");
    for name in events {
        println!("  - {name}");
    }
    Ok(())
}

fn cmd_run(config: &Config, options: GraphStoreOptions) -> Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = Arc::clone(&shutdown);

    ctrlc::set_handler(move || {
        info!("Ctrl+C received — stopping collector");
        shutdown_ctrlc.store(true, Ordering::Relaxed);
    })
    .context("install Ctrl+C handler")?;

    let channel_capacity = config.store.batch_size.max(64);
    let (tx, rx) = sync_channel(channel_capacity);

    let writer_shutdown = Arc::clone(&shutdown);
    let writer_handle = writer::spawn_writer(config.store.clone(), options, rx, writer_shutdown);

    let collector_result = run_collector(config, tx, shutdown);

    match writer_handle.join() {
        Ok(Ok(stats)) => {
            println!(
                "ingested {} event(s), {} node upsert(s), {} edge(s)",
                stats.events, stats.node_upserts, stats.edges_inserted
            );
            println!("graph: {}", config.store.path);
        }
        Ok(Err(e)) => error!(error = %e, "writer thread failed"),
        Err(_) => error!("writer thread panicked"),
    }

    match collector_result {
        Ok(()) => Ok(()),
        Err(EsfError::UnsupportedPlatform) => {
            bail!("live ESF collection requires macOS")
        }
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}
