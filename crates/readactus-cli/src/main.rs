//! Thin CLI harness over readactus-core, for development and testing of
//! the pipeline before the desktop UI exists. Not the product surface.

use std::sync::Arc;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use ddbcore::{ConnectionConfig, EncryptionMode};
use readactus_core::{build_plan, connect_source, connect_target, run_copy, Engine};
use readactus_detect::kind_label;
use readactus_license::{activate, current_entitlements, Entitlements, ISSUER_PUBLIC_KEY_B32};
use readactus_transform::{RunKey, Tokenizer};

#[derive(Parser)]
#[command(name = "readactus", about = "Safe, production-quality database copies without the sensitive data")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone)]
struct DbArgs {
    /// Database engine: postgres | mysql
    #[arg(long)]
    engine: Engine,
    #[arg(long)]
    host: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    database: String,
    #[arg(long)]
    username: String,
    /// Password (or set via environment)
    #[arg(long, env = "READACTUS_DB_PASSWORD", hide_env_values = true)]
    password: String,
}

impl DbArgs {
    fn to_config(&self) -> ConnectionConfig {
        ConnectionConfig {
            host: self.host.clone(),
            port: self.port,
            database: self.database.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            // Clear-text is the free-tier default; TLS is a paid-tier
            // feature and will be wired to licensing when that exists.
            encryption: EncryptionMode::ClearText,
            read_only: false, // source read-only is enforced by readactus-core regardless
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Reflect the source schema and report detected sensitive columns.
    Scan {
        #[command(flatten)]
        source: DbArgs,
        /// Minimum confidence for a finding to be reported
        #[arg(long, default_value_t = 0.5)]
        threshold: f32,
    },
    /// Copy source to target with sensitive columns deterministically transformed.
    Copy {
        #[command(flatten)]
        source: DbArgs,
        /// Target engine: postgres | mysql
        #[arg(long)]
        target_engine: Engine,
        #[arg(long)]
        target_host: String,
        #[arg(long)]
        target_port: u16,
        #[arg(long)]
        target_database: String,
        #[arg(long)]
        target_username: String,
        #[arg(long, env = "READACTUS_TARGET_DB_PASSWORD", hide_env_values = true)]
        target_password: String,
        /// Minimum confidence for a column to be transformed
        #[arg(long, default_value_t = 0.7)]
        threshold: f32,
    },
    /// Activate a Daemon Labs registration key on this computer.
    Activate {
        /// The registration key (RDX1-...)
        key: String,
    },
    /// Show the current license status.
    License,
}

fn load_entitlements() -> Entitlements {
    match current_entitlements(ISSUER_PUBLIC_KEY_B32, None) {
        Ok(ent) => ent,
        Err(e) => {
            // A broken or foreign-machine activation is reported loudly,
            // then the run proceeds on free-tier terms — never silently.
            eprintln!("license warning: {e}; running unregistered (free tier)");
            Entitlements::free()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::Scan { source, threshold } => {
            let conn = connect_source(source.engine, source.to_config()).await?;
            let catalog = conn.reflect_schema().await?;
            let (_, findings) = build_plan(&catalog, threshold);

            let mut reported = 0;
            for finding in &findings {
                if finding.confidence < threshold {
                    continue;
                }
                println!(
                    "{}.{}.{}  [{}]  confidence {:.0}%  — {}",
                    finding.table.schema,
                    finding.table.name,
                    finding.column,
                    kind_label(&finding.kind),
                    finding.confidence * 100.0,
                    finding.reason,
                );
                reported += 1;
            }
            println!("\n{} sensitive column(s) detected across {} schema(s)", reported, catalog.schemas.len());
        }
        Command::Copy {
            source,
            target_engine,
            target_host,
            target_port,
            target_database,
            target_username,
            target_password,
            threshold,
        } => {
            let source_conn = connect_source(source.engine, source.to_config()).await?;
            let target_conn = connect_target(
                target_engine,
                ConnectionConfig {
                    host: target_host,
                    port: target_port,
                    database: target_database,
                    username: target_username,
                    password: target_password,
                    encryption: EncryptionMode::ClearText,
                    read_only: false,
                },
            )
            .await?;

            let entitlements = load_entitlements();
            if let Some(cap) = entitlements.max_extract_bytes {
                println!("Unregistered: extraction is hard-capped at {} MB for this run.", cap / (1024 * 1024));
            }

            let catalog = source_conn.reflect_schema().await?;
            let (plan, findings) = build_plan(&catalog, threshold);
            let transformed_columns = findings.iter().filter(|f| f.confidence >= threshold).count();
            println!("Plan: {} table(s), {} column(s) will be transformed", plan.tables.len(), transformed_columns);

            let tokenizer = Arc::new(Tokenizer::new(RunKey::generate()));
            let report = run_copy(&*source_conn, &*target_conn, &catalog, &plan, tokenizer, &entitlements).await?;

            for table in &report.tables {
                println!(
                    "copied {}.{}: {} row(s), {} column(s) transformed",
                    table.table.schema, table.table.name, table.rows_copied, table.columns_transformed,
                );
            }
            println!("\nDone: {} total row(s) across {} table(s)", report.total_rows, report.tables.len());
        }
        Command::Activate { key } => {
            let activation = activate(&key, ISSUER_PUBLIC_KEY_B32, None)?;
            println!(
                "Activated key {} (tier: {}) on this computer.\n\
                 This key is bound to this machine; it will not work from an activation file copied elsewhere.",
                activation.key_id, activation.tier,
            );
        }
        Command::License => match current_entitlements(ISSUER_PUBLIC_KEY_B32, None) {
            Ok(ent) if ent.registered => println!("Registered. Extraction: unlimited. Bound to this computer."),
            Ok(_) => println!(
                "Unregistered (free tier). Extraction hard-capped at {} MB per run.",
                readactus_license::FREE_TIER_EXTRACT_CAP_BYTES / (1024 * 1024)
            ),
            Err(e) => println!("License problem: {e}"),
        },
    }

    Ok(())
}
