---

# eontree-api

`eontree-api` is the high-performance HTTP service layer for **EonTree**. Built on **Axum** and **Tokio**, it exposes REST/JSON endpoints that allow clients to interact with the decentralized knowledge graph, query decision branches, and submit new propositions through the automated verification and consensus pipeline.

---

# Core Architecture & State Management

The API server runs on a shared state container (`AppState`) wrapped for thread-safe concurrent access across asynchronous request handlers:

1. **Storage Integration (`RedbStore`):** Provides fast, persistent, ACID-compliant access to decision nodes and parent-child adjacency indexes.
2. **Consensus Pipeline (`LLMJudgeClient` & `GlobalSentinel`):** Intercepts incoming node submissions, evaluates ethical constraints against root axioms, and dispatches parallel audits to the Triple-LLM Jury before committing valid records to disk.

---

## API Endpoints

### 1. Retrieve a Node

* **Endpoint:** `GET /nodes/:id`
* **Description:** Fetches a specific `DecisionNode` by its cryptographic Blake3 content-addressable ID.
* **Response:** Returns `200 OK` with the JSON-serialized node, or `404 Not Found`.

### 2. Traverse Child Nodes

* **Endpoint:** `GET /nodes/:id/children`
* **Description:** Queries the underlying graph store to fetch all immediate child nodes linked to the specified parent ID (powering tree exploration).
* **Response:** Returns `200 OK` with a JSON array of `DecisionNode` records.

### 3. Submit a Proposed Node

* **Endpoint:** `POST /nodes`
* **Description:** Ingests a new proposition, runs the Global Sentinel ethical pre-filter, dispatches the parallel Triple-LLM audit, verifies cryptographic signatures, and commits the node to `redb` upon achieving 2/3 consensus.
* **Request Body:** JSON payload representing a signed `DecisionNode`.
* **Response:**
* `201 Created` with consensus metadata upon successful commit.
* `400 Bad Request` if cryptographic verification fails.
* `422 Unprocessable Entity` if rejected by the Global Sentinel or if the 2/3 LLM consensus fails.

---
