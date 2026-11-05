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
}

```

---

## Usage Example

```rust
use eontree_core::{NodeType, NodeId, DecisionNode};
use chrono::Utc;

// 1. Compute a deterministic content ID
let parent_id = None;
let node_type = NodeType::Problem;
let title = "Systemic Resource Scarcity";
let content = "Finite planetary mineral limits constrain infinite exponential extraction models.";
let pubkey = "a1b2c3d4..."; // author Ed25519 public key hex

let id = NodeId::compute(node_type, parent_id.as_ref(), title, content, pubkey);

// 2. Construct the node
let node = DecisionNode {
    id,
    parent_id,
    node_type,
    title: title.to_string(),
    content: content.to_string(),
    evidence_refs: vec!["doi:10.1016/example".into()],
    author_pubkey: pubkey.to_string(),
    author_signature: "sig_hex_string...".to_string(),
    created_at: Utc::now(),
};

// 3. Verify cryptographic and hash integrity
assert!(node.verify().is_ok());

```
