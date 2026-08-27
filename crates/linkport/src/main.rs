//! `linkport` CLI entry point.

mod api;
mod paths;
mod portal;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use linkport_core::engine::{self, Outcome};
use linkport_core::event;
use linkport_core::{config, launcher, TARGET_BLOCK};

#[derive(Parser)]
#[command(
    name = "linkport",
    version,
    about = "Linkport — rule-based browser router with a web portal"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create the default config file (if missing) and print its path.
    Init,
    /// Route a URL through the rules and open it. Hot path used by the OS.
    Open { url: String },
    /// Run the web portal daemon on localhost.
    Serve {
        /// Override the portal port (default: from config, 14200).
        #[arg(long)]
        port: Option<u16>,
    },
    /// Register Linkport as a default-browser candidate (Windows).
    Register {
        /// Path of the executable to register (default: this binary).
        #[arg(long)]
        exe: Option<std::path::PathBuf>,
    },
    /// Remove Linkport's default-browser registration (Windows).
    Unregister,
    /// List configured browsers and browsers discovered on this system.
    Browsers,
    /// Dry-run a URL through the rules and print the decision trace.
    Test { url: String },
    /// Print the portal URL including the auth token.
    PortalUrl,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Init => {
            let path = paths::config_path();
            if !path.exists() {
                config::save(&path, &config::Config::default())
                    .context("failed to write default config")?;
                println!("created {}", path.display());
            } else {
                println!("{}", path.display());
            }
        }
        Commands::Open { url } => open(&url)?,
        Commands::Serve { port } => portal::serve(port)?,
        Commands::Register { exe } => {
            let exe = exe
                .or_else(|| std::env::current_exe().ok())
                .context("could not determine executable path")?;
            linkport_win::register(&exe).map_err(anyhow::Error::msg)?;
            println!("Registered Linkport as a browser candidate.");
            println!("Now open Windows Settings > Apps > Default apps > Linkport and");
            println!("set it as the default for HTTP and HTTPS.");
        }
        Commands::Unregister => {
            linkport_win::unregister().map_err(anyhow::Error::msg)?;
            println!("Unregistered.");
        }
        Commands::Browsers => list_browsers()?,
        Commands::Test { url } => test(&url)?,
        Commands::PortalUrl => {
            let token = paths::ensure_token()?;
            let port = paths::load_or_default().portal.port;
            println!("http://127.0.0.1:{port}/?token={token}");
        }
    }
    Ok(())
}

/// Hot path: read config, evaluate rules, launch the browser, log, exit.
/// Fast and daemon-independent by design.
fn open(url: &str) -> Result<()> {
    let cfg = paths::load_or_default();
    let decision = engine::evaluate(&cfg, url);
    let outcome = decision.outcome.clone();

    let result: Result<()> = match &outcome {
        Outcome::RuleMatched {
            target, incognito, ..
        } => launch_browser(&cfg, target, url, *incognito),
        Outcome::Default { target } => launch_browser(&cfg, target, url, false),
        Outcome::Blocked { .. } => Ok(()),
        Outcome::NoMatch => Err(anyhow::anyhow!(
            "no rule matched and no default browser is configured"
        )),
    };

    // Best-effort history logging; never fail the open because of it.
    let _ = event::append(
        &paths::events_path(),
        &event::Event::new(url, decision.host.clone(), outcome),
    );

    result
}

fn launch_browser(cfg: &config::Config, target: &str, url: &str, incognito: bool) -> Result<()> {
    let browser = cfg
        .browsers
        .get(target)
        .with_context(|| format!("browser '{target}' is not configured"))?;
    launcher::launch(browser, url, incognito)
        .map(|_| ())
        .with_context(|| format!("failed to launch '{}'", browser.exe))
}

fn test(url: &str) -> Result<()> {
    let cfg = paths::load_or_default();
    let decision = engine::evaluate(&cfg, url);
    println!("{}", serde_json::to_string_pretty(&decision)?);
    Ok(())
}

fn list_browsers() -> Result<()> {
    let cfg = paths::load_or_default();
    if cfg.browsers.is_empty() {
        println!("no browsers configured — run `linkport serve` and use the portal");
    } else {
        println!("configured:");
        for (id, b) in &cfg.browsers {
            println!("  {id:<16} {} -> {}", b.display_name, b.exe);
        }
    }
    let discovered = linkport_win::discover_browsers();
    if !discovered.is_empty() {
        println!("discovered on this system:");
        for d in discovered {
            println!("  {:<16} {}", d.name, d.command);
        }
    }
    if let Some(def) = &cfg.default_browser {
        println!("default: {def}");
    } else if !cfg.browsers.is_empty() {
        println!("default: (none set)");
    }
    let _ = TARGET_BLOCK;
    Ok(())
}
