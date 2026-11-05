use clap::{Parser, Subcommand};
use eontree_core::{DecisionNode, NodeType};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "eontree")]
#[command(
    about = "CLI tool for authoring, signing, and submitting EonTree knowledge nodes.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new Ed25519 cryptographic keypair
    Keygen {
        #[arg(short, long, default_value = "eontree_key")]
        output: String,
    },
    /// Author and cryptographically sign a new decision node
    Create {
        #[arg(short, long)]
        title: String,
        #[arg(short, long)]
        content: String,
        #[arg(short, long, default_value = "problem")]
        node_type: String,
        #[arg(short, long)]
        parent_id: String,
        #[arg(short, long)]
        key_file: PathBuf,
        #[arg(short, long, default_value = "node.json")]
        output: PathBuf,
    },
    /// Submit a signed node JSON file to the EonTree API server
    Submit {
        #[arg(short, long, default_value = "node.json")]
        input: PathBuf,
        #[arg(short, long, default_value = "http://127.0.0.1:8080")]
        endpoint: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Keygen { output } => {
            use ed25519_dalek::SigningKey;
            use getrandom::{SysRng, rand_core::UnwrapErr};

            println!("Generating Ed25519 keypair...");

            let mut csprng = UnwrapErr(SysRng);
            let signing_key = SigningKey::generate(&mut csprng);
            let verifying_key = signing_key.verifying_key();

            let pub_path = format!("{}.pub", output);
            let priv_path = format!("{}.priv", output);

            fs::write(&pub_path, hex::encode(verifying_key.to_bytes()))?;
            fs::write(&priv_path, hex::encode(signing_key.to_bytes()))?;

            println!(
                "Keys successfully generated:\n  Public:  {}\n  Private: {}",
                pub_path, priv_path
            );
        }
        Commands::Create {
            title,
            content,
            node_type,
            parent_id,
            key_file,
            output,
        } => {
            // Load the Ed25519 signing key and derive the author's public key from it
            let key_hex = fs::read_to_string(&key_file)?;
            let key_bytes = hex::decode(key_hex.trim())?;
            let key_array: [u8; 32] = key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| "private key file must contain 32 hex-encoded bytes")?;
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_array);
            let author_pubkey = hex::encode(signing_key.verifying_key().to_bytes());

            let n_type = match node_type.to_lowercase().as_str() {
                "root" => NodeType::Root,
                "problem" => NodeType::Problem,
                "unknown" => NodeType::Unknown,
                "barrier" => NodeType::Barrier,
                "innovation" => NodeType::Innovation,
                _ => NodeType::Problem,
            };

            // Wrap parent_id into Option<NodeId> (or None if empty/root)
            let parent_node_id = if parent_id.is_empty() || parent_id.eq_ignore_ascii_case("none") {
                None
            } else {
                Some(eontree_core::NodeId(parent_id))
            };

            // Build node structure with all 6 required parameters
            let mut node = DecisionNode::new(
                parent_node_id,
                n_type,
                title,
                content,
                vec![], // evidence_refs
                author_pubkey,
            );

            // Sign the node's content-addressable ID with the loaded private key
            node.sign(&signing_key);

            // Serialize and save to disk
            let json = serde_json::to_string_pretty(&node)?;
            fs::write(&output, json)?;

            println!(
                "Decision node successfully created and signed!\n  Saved to: {:?}\n  Node ID:  {}",
                output, node.id.0
            );
        }
        Commands::Submit { input, endpoint } => {
            let file_data = fs::read_to_string(&input)?;
            let node: DecisionNode = serde_json::from_str(&file_data)?;

            let client = reqwest::Client::new();
            let url = format!("{}/nodes", endpoint);

            println!("Submitting node {} to {}...", node.id.0, url);

            let res = client.post(&url).json(&node).send().await?;

            if res.status().is_success() {
                println!(
                    "Success! Node accepted and committed by the API server (Status: {}).",
                    res.status()
                );
            } else {
                eprintln!(
                    "Submission rejected by API server (Status: {}).",
                    res.status()
                );
                let err_body = res.text().await?;
                eprintln!("Details: {}", err_body);
            }
        }
    }

    Ok(())
}
