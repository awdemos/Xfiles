use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "xfiles")]
#[command(about = "Plan 9-inspired agent communication hub with quantum-mode AI routing")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the Xfiles daemon.
    Serve,
    /// Print example configuration.
    Config,
    /// Check health of a running Xfiles instance.
    Health {
        #[arg(short, long, default_value = "http://localhost:9999")]
        url: String,
    },
    /// List connected agents.
    Agents {
        #[arg(short, long, default_value = "http://localhost:9999")]
        url: String,
        #[arg(short, long)]
        api_key: Option<String>,
    },
    /// List AI endpoints and their health.
    Endpoints {
        #[arg(short, long, default_value = "http://localhost:9999")]
        url: String,
        #[arg(short, long)]
        api_key: Option<String>,
    },
    /// Fetch Prometheus metrics.
    Metrics {
        #[arg(short, long, default_value = "http://localhost:9999")]
        url: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve => {
            let config = xfiles::config::Config::from_env()?;
            xfiles::daemon::run(config).await?;
        }
        Commands::Config => {
            println!("{}", include_str!("../xfiles.toml.example"));
        }
        Commands::Health { url } => {
            let client = reqwest::Client::new();
            let resp = client.get(format!("{}/health", url.trim_end_matches('/'))).send().await?;
            println!("Status: {}", resp.status());
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Commands::Agents { url, api_key } => {
            let client = reqwest::Client::new();
            let mut req = client.get(format!("{}/agents", url.trim_end_matches('/')));
            if let Some(key) = api_key {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            let resp = req.send().await?;
            println!("Status: {}", resp.status());
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Commands::Endpoints { url, api_key } => {
            let client = reqwest::Client::new();
            let mut req = client.get(format!("{}/endpoints", url.trim_end_matches('/')));
            if let Some(key) = api_key {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            let resp = req.send().await?;
            println!("Status: {}", resp.status());
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Commands::Metrics { url } => {
            let client = reqwest::Client::new();
            let resp = client.get(format!("{}/metrics", url.trim_end_matches('/'))).send().await?;
            println!("Status: {}", resp.status());
            let text = resp.text().await?;
            println!("{}", text);
        }
    }

    Ok(())
}
