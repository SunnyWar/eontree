# EonTree: Workspace Architecture & Development Guide

**EonTree** is an open, community-editable knowledge platform and research laboratory designed to map the "First Principles" of global failure and the "Unsettling Solutions" required to transcend them.

Implemented as a high-performance, deterministic state machine in Rust, EonTree operates as a living decision tree backed by cryptographic verification, ethical pre-filtering via the Global Sentinel, and a distributed Triple-LLM Synthetic Peer-Review jury.

---

## Workspace Crate Topology

The project is structured as a modular Cargo workspace, enforcing strict separation of concerns across core logic, evaluation pipelines, and interfaces:

```text
eontree/
├── Cargo.toml                 # Workspace manifest & shared dependencies
├── crates/
│   ├── eontree-core/          # Domain model, cryptographic verification, and redb storage
│   ├── eontree-jury/          # Global Sentinel ethical filter and Triple-LLM consensus engine
│   ├── eontree-api/           # Axum HTTP service layer and REST endpoints
│   └── eontree-cli/           # Local command-line tool for offline authoring and validation

```

### 1. `eontree-core` (`crates/eontree-core`)

* **Purpose:** Defines the foundational taxonomy (`NodeType`: `Root`, `Problem`, `Unknown`, `Barrier`, `Innovation`), content-addressable identifiers (`NodeId` via Blake3 hashes), and Ed25519 cryptographic provenance verification.
* **Storage:** Implements persistent ACID-compliant storage via **`redb`** (`RedbStore`), maintaining fast parent-child adjacency indexing for graph traversal without external C dependencies.

### 2. `eontree-jury` (`crates/eontree-jury`)

* **Global Sentinel Protocol (`sentinel.rs`):** An ethical pre-filter that evaluates incoming nodes against dynamic graph-defined axioms and constraints to prevent biosphere degradation or non-anthropocentric suffering.
* **Weighted Epistemic Jury (`jury.rs`):** An asynchronous evaluation engine (`tokio` + `reqwest`) that dispatches parallel audits across three independent LLM endpoints and folds their opinions into a single weighted `NodeScorecard`, replacing rigid pass/fail voting with graded-merit scoring. The passing threshold scales with node criticality: `Root`/`Barrier` nodes require a higher composite score than exploratory `Unknown` nodes.

### 3. `eontree-api` (`crates/eontree-api`)

* **Purpose:** A high-performance Axum REST server managing concurrent state via interior mutability (`Mutex<RedbStore>`).
* **Endpoints:** Exposes HTTP routes for fetching nodes (`GET /nodes/:id`), traversing children (`GET /nodes/:id/children`), and submitting cryptographically signed proposals through the validation pipeline (`POST /nodes`).

### 4. `eontree-cli` (`crates/eontree-cli`)

* **Purpose:** Local command-line interface for offline node authoring, cryptographic signing, batch validation, and graph benchmarking.

---

## Getting Started

### Prerequisites

* Rust stable toolchain (edition 2021+)
* Cargo package manager

### Building the Workspace

Clone the repository and compile all workspace crates in release-optimized mode:

```bash
cargo build --release

```

### Running the API Server

Start the Axum web service locally on `[http://127.0.0.1:8080](http://127.0.0.1:8080)`:

```bash
cargo run --bin eontree-api

```