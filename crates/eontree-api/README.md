---

# eontree-api

`eontree-api` is the high-performance HTTP service layer for **EonTree**. Built on **Axum** and **Tokio**, it exposes REST/JSON endpoints that allow clients to interact with the decentralized knowledge graph, query decision branches, and submit new propositions through the automated verification and consensus pipeline.

---

# Core Architecture & State Management

The API server runs on a shared state container (`AppState`) wrapped for thread-safe concurrent access across asynchronous request handlers:

1. **Storage Integration (`RedbStore`):** Provides fast, persistent, ACID-compliant access to decision nodes and parent-child adjacency indexes.
2. **Evaluation Pipeline (`LLMJudgeClient` & `GlobalSentinel`):** Intercepts incoming node submissions, validates structural parent/taxonomic transitions, evaluates ethical constraints against root axioms (hard, binary invariants), and computes a weighted `NodeScorecard` via the Triple-LLM jury before committing approved records to disk.
3. **Automatic Opposition Stubs:** Every approved `Innovation`/`Problem` node automatically spawns three linked adversarial counter-nodes (an objection, a failure mode, and a competing alternative), persisted as `PendingOpposition` stubs signed by the server's own system keypair, ready for a community contributor to claim and flesh out.

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
* **Description:** Ingests a new proposition through a hybrid gate-and-score pipeline: verifies the cryptographic signature, validates the parent exists and the taxonomic transition is legal, runs the Global Sentinel ethical pre-filter, then dispatches a parallel Triple-LLM audit that produces a weighted `NodeScorecard`. The node is committed to `redb` only if its composite score meets the type-scaled passing threshold. If the committed node is an `Innovation` or `Problem`, the server automatically spawns and persists three linked `PendingOpposition` counter-node stubs (objection, failure mode, competing alternative) and records their IDs on the parent's `counter_node_ids`.
* **Request Body:** JSON payload representing a signed `DecisionNode`.
* **Response:**
* `201 Created` upon successful commit.
* `400 Bad Request` if cryptographic verification fails (hard invariant).
* `404 Not Found` if `parent_id` is set but no such node exists.
* `422 Unprocessable Entity` if the taxonomic transition is invalid, if rejected by the Global Sentinel (hard invariants), or if the weighted jury score falls below the passing threshold — in the jury-rejection case the response body contains the `NodeScorecard` detailing each judge's weighted criterion for auditability.

### 4. List the Opposition "Todo Queue"

* **Endpoint:** `GET /nodes/todos`
* **Description:** Lists every node currently in `PendingOpposition` status — the auto-generated adversarial stubs awaiting a community contributor to flesh out and claim.
* **Response:** Returns `200 OK` with a JSON array of `DecisionNode` records.

---
