//! `linkport` CLI entry point (console binary).

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use linkport_core::config;

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
    /// Route a URL through the rules and open it. Console hot path
    /// (the OS-facing handler is linkport-open, which has no console).
    Open { url: String },
    /// Run the web portal daemon on localhost.
    Serve {
        /// Override the portal port (default: from config, 14200).
        #[arg(long)]
        port: Option<u16>,
        /// Open the portal in the system browser after starting.
        #[arg(long)]
        open: bool,
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
            let path = linkport::paths::config_path();
            if !path.exists() {
                config::save(&path, &config::Config::default())
                    .context("failed to write default config")?;
                println!("created {}", path.display());
            } else {
                println!("{}", path.display());
            }
        }
        Commands::Open { url } => linkport::open::open_url(&url)?,
        Commands::Serve { port, open } => linkport::portal::serve(port, open)?,
        Commands::Register { exe } => {
            let exe = exe
                .or_else(|| std::env::current_exe().ok())
                .context("could not determine executable path")?;
            let handler = linkport::default_handler_command();
            linkport_win::register(&exe, &handler).map_err(anyhow::Error::msg)?;
            println!("Registered Linkport as a browser candidate.");
            println!("Handler: {handler}");
            println!("Now open Windows Settings > Apps > Default apps > Linkport and");
            println!("set it as the default for HTTP and HTTPS.");
        }
        Commands::Unregister => {
            linkport_win::unregister().map_err(anyhow::Error::msg)?;
            println!("Unregistered.");
        }
        Commands::Browsers => linkport::open::list_browsers()?,
        Commands::Test { url } => linkport::open::test_url(&url)?,
        Commands::PortalUrl => {
            let token = linkport::paths::ensure_token()?;
            let port = linkport::paths::load_or_default().portal.port;
            println!("http://127.0.0.1:{port}/?token={token}");
        }
    }
    Ok(())
}
