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
    /// List auto-generated adversarial stubs (objections, failure modes,
    /// competing alternatives) still waiting for a contributor to flesh out
    ListTodos {
        #[arg(short, long, default_value = "http://127.0.0.1:8080")]
        endpoint: String,
    },
    /// Claim a pending-opposition stub: fetch it, flesh out real content, sign
    /// with your own key, and submit the fleshed-out node to the server
    Claim {
        /// NodeId of the stub to claim (see `list-todos`)
        #[arg(short, long)]
        todo_id: String,
        #[arg(short, long)]
        content: String,
        /// Comma-separated evidence references (URIs, DOIs, citation hashes)
        #[arg(short = 'v', long, default_value = "")]
        evidence: String,
        #[arg(short, long)]
        key_file: PathBuf,
        #[arg(short, long, default_value = "http://127.0.0.1:8080")]
        endpoint: String,
        #[arg(short, long, default_value = "claimed_node.json")]
        output: PathBuf,
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
        Commands::ListTodos { endpoint } => {
            let client = reqwest::Client::new();
            let url = format!("{}/nodes/todos", endpoint);

            let res = client.get(&url).send().await?;

            if res.status().is_success() {
                let todos: Vec<DecisionNode> = res.json().await?;
                if todos.is_empty() {
                    println!("No pending-opposition stubs. The todo queue is empty!");
                } else {
                    println!("Pending opposition stubs ({}):", todos.len());
                    for node in &todos {
                        let parent = node
                            .parent_id
                            .as_ref()
                            .map(|p| p.0.as_str())
                            .unwrap_or("none");
                        println!(
                            "  [{:?}] {}
    id:     {}
    parent: {}",
                            node.node_type, node.title, node.id.0, parent
                        );
                    }
                }
            } else {
                eprintln!("Failed to fetch todos (Status: {}).", res.status());
            }
        }
        Commands::Claim {
            todo_id,
            content,
            evidence,
            key_file,
            endpoint,
            output,
        } => {
            let client = reqwest::Client::new();

            // 1. Fetch the stub to discover its parent/type/title context.
            let stub_url = format!("{}/nodes/{}", endpoint, todo_id);
            let stub_res = client.get(&stub_url).send().await?;
            if !stub_res.status().is_success() {
                eprintln!(
                    "Could not fetch todo stub {} (Status: {}).",
                    todo_id,
                    stub_res.status()
                );
                return Ok(());
            }
            let stub: DecisionNode = stub_res.json().await?;

            // 2. Load the contributor's signing key.
            let key_hex = fs::read_to_string(&key_file)?;
            let key_bytes = hex::decode(key_hex.trim())?;
            let key_array: [u8; 32] = key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| "private key file must contain 32 hex-encoded bytes")?;
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_array);
            let author_pubkey = hex::encode(signing_key.verifying_key().to_bytes());

            let evidence_refs: Vec<String> = evidence
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();

            // 3. Build and sign a fully fleshed-out Active node reusing the
            // stub's parent and taxonomic type. This is a new, independently
            // signed node (content-addressable IDs can't be reused across
            // authors/content); the original stub remains in the graph as a
            // historical PendingOpposition record.
            let mut node = DecisionNode::new(
                stub.parent_id.clone(),
                stub.node_type,
                stub.title.clone(),
                content,
                evidence_refs,
                author_pubkey,
            );
            node.sign(&signing_key);

            let json = serde_json::to_string_pretty(&node)?;
            fs::write(&output, &json)?;

            // 4. Submit the claimed node to the server.
            let submit_url = format!("{}/nodes", endpoint);
            let submit_res = client.post(&submit_url).json(&node).send().await?;

            if submit_res.status().is_success() {
                println!(
                    "Claimed todo {}! New node {} saved to {:?} and accepted by the server (Status: {}).",
                    todo_id,
                    node.id.0,
                    output,
                    submit_res.status()
                );
            } else {
                eprintln!(
                    "Submission rejected by API server (Status: {}).",
                    submit_res.status()
                );
                let err_body = submit_res.text().await?;
                eprintln!("Details: {}", err_body);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keygen_with_default_output() {
        let cli = Cli::try_parse_from(["eontree", "keygen"]).unwrap();
        match cli.command {
            Commands::Keygen { output } => assert_eq!(output, "eontree_key"),
            _ => panic!("expected Keygen command"),
        }
    }

    #[test]
    fn parses_keygen_with_custom_output() {
        let cli = Cli::try_parse_from(["eontree", "keygen", "--output", "mykey"]).unwrap();
        match cli.command {
            Commands::Keygen { output } => assert_eq!(output, "mykey"),
            _ => panic!("expected Keygen command"),
        }
    }

    #[test]
    fn parses_create_with_required_args_and_defaults() {
        let cli = Cli::try_parse_from([
            "eontree",
            "create",
            "--title",
            "T",
            "--content",
            "C",
            "--parent-id",
            "none",
            "--key-file",
            "key.priv",
        ])
        .unwrap();

        match cli.command {
            Commands::Create {
                title,
                content,
                node_type,
                parent_id,
                key_file,
                output,
            } => {
                assert_eq!(title, "T");
                assert_eq!(content, "C");
                assert_eq!(node_type, "problem");
                assert_eq!(parent_id, "none");
                assert_eq!(key_file, PathBuf::from("key.priv"));
                assert_eq!(output, PathBuf::from("node.json"));
            }
            _ => panic!("expected Create command"),
        }
    }

    #[test]
    fn create_missing_required_args_fails_to_parse() {
        let result = Cli::try_parse_from(["eontree", "create", "--title", "T"]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_submit_with_defaults() {
        let cli = Cli::try_parse_from(["eontree", "submit"]).unwrap();
        match cli.command {
            Commands::Submit { input, endpoint } => {
                assert_eq!(input, PathBuf::from("node.json"));
                assert_eq!(endpoint, "http://127.0.0.1:8080");
            }
            _ => panic!("expected Submit command"),
        }
    }

    #[test]
    fn parses_submit_with_custom_endpoint() {
        let cli =
            Cli::try_parse_from(["eontree", "submit", "--endpoint", "http://example.com"]).unwrap();
        match cli.command {
            Commands::Submit { endpoint, .. } => assert_eq!(endpoint, "http://example.com"),
            _ => panic!("expected Submit command"),
        }
    }

    #[test]
    fn parses_list_todos_with_defaults() {
        let cli = Cli::try_parse_from(["eontree", "list-todos"]).unwrap();
        match cli.command {
            Commands::ListTodos { endpoint } => assert_eq!(endpoint, "http://127.0.0.1:8080"),
            _ => panic!("expected ListTodos command"),
        }
    }

    #[test]
    fn parses_claim_with_required_args_and_defaults() {
        let cli = Cli::try_parse_from([
            "eontree",
            "claim",
            "--todo-id",
            "stub-hash",
            "--content",
            "Real argument with citations",
            "--key-file",
            "key.priv",
        ])
        .unwrap();

        match cli.command {
            Commands::Claim {
                todo_id,
                content,
                evidence,
                key_file,
                endpoint,
                output,
            } => {
                assert_eq!(todo_id, "stub-hash");
                assert_eq!(content, "Real argument with citations");
                assert_eq!(evidence, "");
                assert_eq!(key_file, PathBuf::from("key.priv"));
                assert_eq!(endpoint, "http://127.0.0.1:8080");
                assert_eq!(output, PathBuf::from("claimed_node.json"));
            }
            _ => panic!("expected Claim command"),
        }
    }

    #[test]
    fn parses_claim_with_evidence_list() {
        let cli = Cli::try_parse_from([
            "eontree",
            "claim",
            "--todo-id",
            "stub-hash",
            "--content",
            "Real argument",
            "--evidence",
            "doi:10.1/example, https://example.com",
            "--key-file",
            "key.priv",
        ])
        .unwrap();

        match cli.command {
            Commands::Claim { evidence, .. } => {
                assert_eq!(evidence, "doi:10.1/example, https://example.com");
            }
            _ => panic!("expected Claim command"),
        }
    }

    #[test]
    fn claim_missing_required_args_fails_to_parse() {
        let result = Cli::try_parse_from(["eontree", "claim", "--todo-id", "stub-hash"]);
        assert!(result.is_err());
    }
}
