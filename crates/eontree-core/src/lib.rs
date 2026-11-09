
pub mod store;

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

/// Lifecycle status of a node. This is administrative metadata, not part of the
/// content-addressable payload: toggling it does not change a node's NodeId and
/// therefore never invalidates its author's signature (which covers only the ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Active,
    /// Automatically generated adversarial stub waiting for community critique.
    PendingOpposition,
}
/// Content-addressable identifier derived from a Blake3 hash of the node's immutable fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);
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
    pub status: NodeStatus,
    /// IDs of adversarial counter-nodes (objections, failure modes, alternatives)
    /// automatically spawned alongside this node.
    pub counter_node_ids: Vec<NodeId>,
}

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

impl DecisionNode {
    /// Creates a new decision node, automatically computing its Blake3 content-addressable hash.
    pub fn new(
        parent_id: Option<NodeId>,
        node_type: NodeType,
        title: String,
        content: String,
        evidence_refs: Vec<String>,
        author_pubkey: String,
    ) -> Self {
        let created_at = chrono::Utc::now();

        // Compute the content-addressable NodeId, matching the fields checked by `verify`.
        let id = NodeId::compute(
            node_type,
            parent_id.as_ref(),
            &title,
            &content,
            &author_pubkey,
        );

        Self {
            id,
            parent_id,
            node_type,
            title,
            content,
            evidence_refs,
            author_pubkey,
            author_signature: String::new(), // Populated by `sign`
            created_at,
            status: NodeStatus::Active,
            counter_node_ids: Vec::new(),
        }
    }

    /// Signs the node's content-addressable ID with the given Ed25519 signing key,
    /// populating `author_signature`. The caller must ensure `author_pubkey` already
    /// matches `signing_key`, since it is baked into the ID that gets signed.
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let signature = signing_key.sign(self.id.0.as_bytes());
        self.author_signature = hex::encode(signature.to_bytes());
    }
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
        let pubkey_bytes =
            hex::decode(&self.author_pubkey).map_err(|_| CoreError::InvalidSignature)?;
        let sig_bytes =
            hex::decode(&self.author_signature).map_err(|_| CoreError::InvalidSignature)?;

        let public_key = ed25519_dalek::VerifyingKey::from_bytes(
            pubkey_bytes
                .as_slice()
                .try_into()
                .map_err(|_| CoreError::InvalidSignature)?,
        )
        .map_err(|_| CoreError::InvalidSignature)?;

        let signature = ed25519_dalek::Signature::from_bytes(
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| CoreError::InvalidSignature)?,
        );

        // Sign over the hash ID string payload
        use ed25519_dalek::Verifier;
        public_key
            .verify(self.id.0.as_bytes(), &signature)
            .map_err(|_| CoreError::InvalidSignature)?;

        Ok(())
    }
}

/// Graph storage abstraction trait for database backends (e.g., SQLite / sled / Redb).
pub trait GraphStore {
    type Error: std::fmt::Display;

    fn get_node(&self, id: &NodeId) -> Result<Option<DecisionNode>, Self::Error>;
    fn put_node(&mut self, node: &DecisionNode) -> Result<(), Self::Error>;
    fn get_children(&self, id: &NodeId) -> Result<Vec<DecisionNode>, Self::Error>;
    /// Lists every node currently in the given lifecycle status (e.g., the
    /// community-opposition "todo queue" of `PendingOpposition` stubs).
    fn list_by_status(&self, status: NodeStatus) -> Result<Vec<DecisionNode>, Self::Error>;
}


#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn signed_node(key: &SigningKey) -> DecisionNode {
        let author_pubkey = hex::encode(key.verifying_key().to_bytes());
        let mut node = DecisionNode::new(
            None,
            NodeType::Root,
            "Title".into(),
            "Content".into(),
            vec![],
            author_pubkey,
        );
        node.sign(key);
        node
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let node = signed_node(&test_key());
        node.verify().expect("freshly signed node should verify");
    }

    #[test]
    fn tampered_content_fails_verification() {
        let mut node = signed_node(&test_key());
        node.content = "Tampered content".into();
        assert!(matches!(node.verify(), Err(CoreError::HashMismatch { .. })));
    }

    #[test]
    fn tampered_signature_fails_verification() {
        let mut node = signed_node(&test_key());
        node.author_signature = hex::encode([0u8; 64]);
        assert!(matches!(node.verify(), Err(CoreError::InvalidSignature)));
    }

    #[test]
    fn wrong_key_signature_fails_verification() {
        let node = signed_node(&test_key());
        let other_key = SigningKey::from_bytes(&[9u8; 32]);
        let mut forged = node.clone();
        forged.sign(&other_key);
        assert!(matches!(forged.verify(), Err(CoreError::InvalidSignature)));
    }

    #[test]
    fn status_and_counter_node_ids_are_not_part_of_the_signed_payload() {
        let mut node = signed_node(&test_key());
        node.status = NodeStatus::PendingOpposition;
        node.counter_node_ids = vec![NodeId("some-counter-node".into())];

        node.verify()
            .expect("administrative metadata should not affect verification");
    }
}
