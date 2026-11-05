---

# eontree-core

`eontree-core` is the foundational domain model, cryptographic verification engine, and storage abstraction layer for **EonTree**. It defines the deterministic state machine and directed acyclic graph (DAG) primitives that power the knowledge platform.

---

# Core Architecture

To guarantee absolute data integrity, auditability, and tamper-resistance across a decentralized, community-editable platform, `eontree-core` enforces strict design principles:

1. **Content-Addressable Immutability:** Nodes are not identified by mutable database primary keys. Instead, every node's identity (`NodeId`) is derived from a cryptographic **Blake3** hash of its immutable taxonomic payload, parent relationship, and author public key. Edits or refinements generate new immutable child nodes rather than in-place overwrites.
2. **Cryptographic Provenance:** Every proposition, problem definition, and solution pathway must be signed by its author using **Ed25519** signatures. The verification engine ensures that no node can enter the graph without valid cryptographic proof of authorship.
3. **Rigorous Taxonomy:** The graph enforces structural clarity by categorizing every proposition into a distinct taxonomic node type.

---

## Taxonomic Types (`NodeType`)

* **`Root`:** The foundational axioms and baseline framework parameters.
* **`Problem`:** Fundamental constraints of physics, biology, economics, and game theory.
* **`Unknown`:** Explicitly identified gaps in human knowledge or empirical data.
* **`Barrier`:** Specific economic, political, psychological, or systemic hurdles preventing progress.
* **`Innovation`:** Logical solution pathways, analyzed objectively regardless of convention.

---

## Data Model & API Overview

### `DecisionNode`

The core atomic unit of the EonTree knowledge graph:

```rust
pub struct DecisionNode {
    pub id: NodeId,                           // Blake3 content-addressable hash
    pub parent_id: Option<NodeId>,            // Causal parent node
    pub node_type: NodeType,                  // Root, Problem, Unknown, Barrier, Innovation
    pub title: String,                        // Concise summary title
    pub content: String,                      // Detailed logical argument and evidence
    pub evidence_refs: Vec<String>,           // External URIs, DOIs, or citation hashes
    pub author_pubkey: String,                // Hex-encoded Ed25519 public key
    pub author_signature: String,             // Hex-encoded Ed25519 signature over node ID
    pub created_at: DateTime<Utc>,            // UTC timestamp
    pub status: NodeStatus,                   // Active or PendingOpposition
    pub counter_node_ids: Vec<NodeId>,        // Linked adversarial counter-nodes, if any
}

```

`status` and `counter_node_ids` are administrative lifecycle metadata, not part of the
immutable content payload: they are excluded from both the Blake3 hash and the signed
payload, so updating them never invalidates a node's `NodeId` or its author's signature.

### `NodeStatus`

```rust
pub enum NodeStatus {
    Active,
    PendingOpposition, // Automatically generated stub awaiting community critique
}

```

### Integrity Verification

Calling `.verify()` on any `DecisionNode` executes a two-step validation check:

1. **Hash Re-computation:** Recalculates the Blake3 hash from the payload fields and asserts it matches the node's `NodeId`.
2. **Cryptographic Signature Check:** Decodes the author's Ed25519 public key and verifies the cryptographic signature against the node identifier payload.

---

## Storage Abstraction (`GraphStore`)

The crate defines a clean persistence trait (`GraphStore`) allowing the core engine to remain agnostic to the underlying database backend (e.g., SQLite, `sled`, `redb`, or in-memory test stores):

```rust
pub trait GraphStore {
    type Error;

    fn get_node(&self, id: &NodeId) -> Result<Option<DecisionNode>, Self::Error>;
    fn put_node(&mut self, node: &DecisionNode) -> Result<(), Self::Error>;
    fn get_children(&self, id: &NodeId) -> Result<Vec<DecisionNode>, Self::Error>;
    fn list_by_status(&self, status: NodeStatus) -> Result<Vec<DecisionNode>, Self::Error>;
}

```

---

## Usage Example

```rust
use eontree_core::{DecisionNode, NodeType};
use ed25519_dalek::SigningKey;

// 1. Load or generate the author's Ed25519 signing key
let signing_key = SigningKey::from_bytes(&[7u8; 32]);
let author_pubkey = hex::encode(signing_key.verifying_key().to_bytes());

// 2. Construct the node; `DecisionNode::new` computes its content-addressable
// NodeId automatically and defaults status to Active with no counter-nodes.
let mut node = DecisionNode::new(
    None, // parent_id
    NodeType::Problem,
    "Systemic Resource Scarcity".to_string(),
    "Finite planetary mineral limits constrain infinite exponential extraction models.".to_string(),
    vec!["doi:10.1016/example".into()],
    author_pubkey,
);

// 3. Sign the node's ID with the author's key
node.sign(&signing_key);

// 4. Verify cryptographic and hash integrity
assert!(node.verify().is_ok());

```
