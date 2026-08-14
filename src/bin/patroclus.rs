use clap::{Parser, Subcommand};
use patroclus::config::Config;

#[derive(Parser)]
#[command(name = "patroclus")]
#[command(about = "Scoped, time-limited authorization infrastructure for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new configuration file
    Init,
    /// Start the server
    Serve {
        #[arg(short, long, default_value = "config.toml")]
        config: String,
    },
    /// Generate a new RSA keypair for token signing
    GenerateKeys {
        #[arg(short, long, default_value = "keys")]
        output_dir: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "patroclus=debug,tower_http=debug".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let config = Config::default();
            let toml_str = toml::to_string_pretty(&config)?;
            std::fs::write("config.toml", toml_str)?;
            println!("Created config.toml");
        }
        Commands::Serve { config } => {
            let config = Config::load(&config)?;
            patroclus::api::server::run(config).await?;
        }
        Commands::GenerateKeys { output_dir } => {
            std::fs::create_dir_all(&output_dir)?;
            patroclus::token::issuer::generate_keypair(&output_dir)?;
            println!("Generated RSA keypair in {}", output_dir);
        }
    }

    Ok(())
}
