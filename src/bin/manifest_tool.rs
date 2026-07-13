//! Offline tooling for the managed-engine manifest release pipeline:
//! computing the checksum/size of a downloaded artifact, generating an
//! Ed25519 signing keypair, and signing/verifying a manifest. Keeps the
//! private key out of the main crate entirely — this binary is the only
//! place that ever touches it.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use ravyn::services::engines::{EngineManifest, SignedEngineManifest};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(
    name = "manifest-tool",
    about = "Generate, checksum, sign, and verify Ravyn engine manifests"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the SHA-256 and size in bytes of a local file, for filling in
    /// an EngineArtifact's `sha256`/`size_bytes` (or `member_sha256` for an
    /// extracted archive member) by hand.
    Checksum { file: PathBuf },
    /// Generate a new Ed25519 signing keypair. The private key never touches
    /// disk unless `--out` is given; prefer piping it straight into a secret
    /// store.
    Keygen {
        /// Write "<private_hex> <public_hex>" to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Sign an unsigned manifest (schema_version/channel/artifacts, no
    /// signature field) with a private key, producing a SignedEngineManifest.
    Sign {
        /// Path to the unsigned manifest JSON.
        #[arg(long)]
        manifest: PathBuf,
        /// 64-character hex Ed25519 private key seed. Falls back to the
        /// RAVYN_ENGINE_MANIFEST_PRIVATE_KEY environment variable so it
        /// never has to appear in shell history or CI logs.
        #[arg(long)]
        key: Option<String>,
        /// Where to write the signed manifest JSON.
        #[arg(long)]
        out: PathBuf,
    },
    /// Verify a signed manifest against a public key, exactly as the backend
    /// would when loading it.
    Verify {
        /// Path to the signed manifest JSON.
        signed: PathBuf,
        /// 64-character hex Ed25519 public key.
        #[arg(long)]
        public_key: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Checksum { file } => checksum(&file),
        Command::Keygen { out } => keygen(out.as_deref()),
        Command::Sign { manifest, key, out } => sign(&manifest, key, &out),
        Command::Verify { signed, public_key } => verify(&signed, &public_key),
    }
}

fn checksum(file: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(file)?;
    println!("sha256:     {}", hex::encode(Sha256::digest(&bytes)));
    println!("size_bytes: {}", bytes.len());
    Ok(())
}

fn keygen(out: Option<&std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = [0_u8; 32];
    getrandom(&mut seed)?;
    let signing_key = SigningKey::from_bytes(&seed);
    let private_hex = hex::encode(signing_key.to_bytes());
    let public_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let line = format!("{private_hex} {public_hex}");
    match out {
        Some(path) => std::fs::write(path, format!("{line}\n"))?,
        None => println!("{line}"),
    }
    eprintln!(
        "Keep the first (private) value secret; embed the second (public) value as \
         RAVYN_ENGINE_MANIFEST_PUBLIC_KEY at build time."
    );
    Ok(())
}

fn sign(
    manifest_path: &std::path::Path,
    key: Option<String>,
    out: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest: EngineManifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
    manifest.validate()?;

    let key_hex = key
        .or_else(|| std::env::var("RAVYN_ENGINE_MANIFEST_PRIVATE_KEY").ok())
        .ok_or("no private key: pass --key or set RAVYN_ENGINE_MANIFEST_PRIVATE_KEY")?;
    let key_bytes: [u8; 32] = hex::decode(key_hex.trim())?
        .try_into()
        .map_err(|_| "private key must be exactly 32 bytes (64 hex characters)")?;
    let signing_key = SigningKey::from_bytes(&key_bytes);

    let payload = serde_json::to_vec(&manifest)?;
    let signature = signing_key.sign(&payload);
    let signed = SignedEngineManifest {
        manifest,
        signature: hex::encode(signature.to_bytes()),
    };

    // Round-trip through the same verification path the backend uses, so a
    // signature that doesn't actually verify never reaches an output file.
    signed.verify(&signing_key.verifying_key().to_bytes())?;

    std::fs::write(out, serde_json::to_vec_pretty(&signed)?)?;
    println!(
        "signed {} artifact(s) -> {}",
        signed.manifest.artifacts.len(),
        out.display()
    );
    Ok(())
}

fn verify(
    signed_path: &std::path::Path,
    public_key_hex: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let signed: SignedEngineManifest = serde_json::from_slice(&std::fs::read(signed_path)?)?;
    let public_key: [u8; 32] = hex::decode(public_key_hex.trim())?
        .try_into()
        .map_err(|_| "public key must be exactly 32 bytes (64 hex characters)")?;
    let manifest = signed.verify(&public_key)?;
    println!(
        "OK: {} channel, {} artifact(s), signature verified",
        manifest.channel,
        manifest.artifacts.len()
    );
    Ok(())
}

/// Cryptographically random bytes without adding a new dependency: reads
/// directly from the OS CSPRNG device/API that `ed25519-dalek`'s own
/// optional `rand_core` feature would otherwise pull in.
#[cfg(windows)]
fn getrandom(buffer: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("BCryptGenRandom failed with status {status:#x}").into())
    }
}

#[cfg(unix)]
fn getrandom(buffer: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open("/dev/urandom")?;
    std::io::Read::read_exact(&mut file, buffer)?;
    Ok(())
}
