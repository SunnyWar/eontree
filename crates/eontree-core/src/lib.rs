use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Cryptographic signature verification failed")]
    InvalidSignature,
    #[error("Node hash mismatch: expected {expected}, computed {computed}")]
    HashMismatch { expected: String, computed: String },
    #[error("Graph validation error: {0}")]
    GraphValidation(String),
}

/// The fundamental taxonomic categories of the EonTree knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Root,
    Problem,
    Unknown,
    Barrier,
    Innovation,
}

/// Content-addressable identifier derived from a Blake3 hash of the node's immutable fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    /// Computes a deterministic content-addressable ID from node payload fields.
    pub fn compute(
        node_type: NodeType,
        parent_id: Option<&NodeId>,
        title: &str,
        content: &str,
        author_pubkey: &str,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(format!("{:?}", node_type).as_bytes());
        if let Some(pid) = parent_id {
            hasher.update(pid.0.as_bytes());
        }
        hasher.update(title.as_bytes());
        hasher.update(content.as_bytes());
        hasher.update(author_pubkey.as_bytes());
        
        Self(hasher.finalize().to_hex().to_string())
    }
}

/// Relationship types between nodes in the decision tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    /// Connects a parent node to its taxonomic sub-component.
    Subdivision,
    /// Connects an Innovation directly addressing a Problem or Barrier.
    Mitigates,
    /// Connects an Unknown or Barrier highlighting a flaw or gap.
    Exposes,
    /// Evidentiary support link.
    Evidences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub node_type: NodeType,
    pub title: String,
    pub content: String,
    pub evidence_refs: Vec<String>,
    pub author_pubkey: String,
    pub author_signature: String,
    pub created_at: DateTime<Utc>,
}

impl DecisionNode {
    /// Verifies the structural integrity and cryptographic signature of the node.
    pub fn verify(&self) -> Result<(), CoreError> {
        // 1. Verify content-addressable ID
        let expected_id = NodeId::compute(
            self.node_type,
            self.parent_id.as_ref(),
            &self.title,
            &self.content,
            &self.author_pubkey,
        );
        
        if self.id != expected_id {
            return Err(CoreError::HashMismatch {
                expected: expected_id.0,
                computed: self.id.0.clone(),
            });
        }

        // 2. Verify Ed25519 signature
        let pubkey_bytes = hex::decode(&self.author_pubkey)
            .map_err(|_| CoreError::InvalidSignature)?;
        let sig_bytes = hex::decode(&self.author_signature)
            .map_err(|_| CoreError::InvalidSignature)?;
            
        let public_key = ed25519_dalek::VerifyingKey::from_bytes(
            pubkey_bytes.as_slice().try_into().map_err(|_| CoreError::InvalidSignature)?
        ).map_err(|_| CoreError::InvalidSignature)?;
        
        let signature = ed25519_dalek::Signature::from_bytes(
            sig_bytes.as_slice().try_into().map_err(|_| CoreError::InvalidSignature)?
        );

        // Sign over the hash ID string payload
        use ed25519_dalek::Verifier;
        public_key.verify(self.id.0.as_bytes(), &signature)
            .map_err(|_| CoreError::InvalidSignature)?;

        Ok(())
    }
}

/// Graph storage abstraction trait for database backends (e.g., SQLite / sled / Redb).
pub trait GraphStore {
    type Error;

    fn get_node(&self, id: &NodeId) -> Result<Option<DecisionNode>, Self::Error>;
    fn put_node(&mut self, node: &DecisionNode) -> Result<(), Self::Error>;
    fn get_children(&self, id: &NodeId) -> Result<Vec<DecisionNode>, Self::Error>;
}