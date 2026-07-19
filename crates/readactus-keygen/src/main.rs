//! Daemon Labs INTERNAL key issuer. This binary must never be shipped to
//! users or included in release artifacts — whoever holds the issuer
//! secret can mint valid registration keys.
//!
//! Workflow:
//!   readactus-keygen init            # once: writes issuer.secret (gitignored),
//!                                    # prints the public key to embed in
//!                                    # readactus-license::ISSUER_PUBLIC_KEY_B32
//!   readactus-keygen issue --tier pro  # per customer: prints one key

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signer, SigningKey};
use readactus_license::KeyPayload;

#[derive(Parser)]
#[command(name = "readactus-keygen", about = "Daemon Labs internal registration-key issuer")]
struct Cli {
    /// Path to the issuer secret key file
    #[arg(long, default_value = "issuer.secret")]
    secret_file: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a new issuer keypair. Refuses to overwrite an existing secret.
    Init,
    /// Issue one registration key.
    Issue {
        /// Tier recorded in the key (e.g. pro)
        #[arg(long, default_value = "pro")]
        tier: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => {
            if cli.secret_file.exists() {
                bail!(
                    "{} already exists — refusing to overwrite the issuer secret. \
                     Rotating the issuer key invalidates every key ever issued.",
                    cli.secret_file.display()
                );
            }
            let signing = SigningKey::generate(&mut rand::rngs::OsRng);
            let secret_b32 = BASE32_NOPAD.encode(signing.as_bytes());
            let public_b32 = BASE32_NOPAD.encode(signing.verifying_key().as_bytes());

            std::fs::write(&cli.secret_file, &secret_b32)
                .with_context(|| format!("writing {}", cli.secret_file.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cli.secret_file, std::fs::Permissions::from_mode(0o600))?;
            }

            println!("issuer secret written to {} (mode 0600 — keep it OUT of git and backups you don't control)", cli.secret_file.display());
            println!();
            println!("Embed this public key as readactus_license::ISSUER_PUBLIC_KEY_B32:");
            println!("{public_b32}");
        }
        Command::Issue { tier } => {
            let secret_b32 = std::fs::read_to_string(&cli.secret_file)
                .with_context(|| format!("reading {} — run `readactus-keygen init` first", cli.secret_file.display()))?;
            let secret_bytes = BASE32_NOPAD
                .decode(secret_b32.trim().as_bytes())
                .context("issuer secret is not valid base32")?;
            let secret_arr: [u8; 32] = secret_bytes.try_into().ok().context("issuer secret has wrong length")?;
            let signing = SigningKey::from_bytes(&secret_arr);

            let payload = KeyPayload { version: 1, key_id: uuid::Uuid::new_v4().to_string(), tier };
            let payload_bytes = serde_json::to_vec(&payload)?;
            let signature = signing.sign(&payload_bytes);

            println!(
                "RDX1-{}-{}",
                BASE32_NOPAD.encode(&payload_bytes),
                BASE32_NOPAD.encode(&signature.to_bytes())
            );
        }
    }
    Ok(())
}
