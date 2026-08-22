use clap::{Parser, Subcommand};
use patroclus::audit::verify_chain;
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
    /// Recompute the SHA-256 hash chain over the audit log and report the
    /// first broken link (tamper detection)
    VerifyChain {
        /// Path to the SQLite database holding `audit_log`
        #[arg(short, long, default_value = "patroclus.db")]
        db: String,
        /// Emit the verification result as JSON instead of text
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let json_logs = std::env::var("PATROCLUS_LOG_FORMAT").as_deref() == Ok("json");
    let subscriber = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "patroclus=debug,tower_http=debug".into()),
    );
    if json_logs {
        subscriber.json().init();
    } else {
        subscriber.init();
    }

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
        Commands::VerifyChain { db, json } => {
            // Open read-only so tamper inspection can never mutate evidence.
            let conn = rusqlite::Connection::open_with_flags(
                &db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let entries = patroclus::db::read_audit_entries_for_verification(&conn)?;
            let result = verify_chain(&entries);

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
                if result.is_valid() {
                    std::process::exit(0);
                }
                std::process::exit(1);
            }

            match result.first_broken_link {
                None => {
                    println!(
                        "OK: audit chain verified over {} entries",
                        result.entries_checked
                    );
                }
                Some(link) => {
                    eprintln!(
                        "FAILED: audit chain broken at row {} ({} of {} checked): {}",
                        link.row_id,
                        result.entries_checked,
                        entries.len(),
                        match link.reason {
                            patroclus::audit::BrokenLinkReason::RowHashMismatch =>
                                "stored row_hash does not match recomputed row payload \
                                 (row was modified)",
                            patroclus::audit::BrokenLinkReason::PrevHashMismatch => {
                                "prev_hash does not chain to the previous row_hash \
                                 (rows were deleted, inserted or reordered)"
                            }
                        }
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
