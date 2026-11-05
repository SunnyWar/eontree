---

# eontree-cli

`eontree-cli` is the local command-line companion for **EonTree**. It provides offline authoring capabilities, Ed25519 keypair management, cryptographic content-addressable hashing (via Blake3), and a lightweight client to submit signed decision nodes to a running `eontree-api` server.

---

# Commands & Usage

## 1. Generate Cryptographic Keypairs (`keygen`)

Generates Ed25519 public and private key files used to sign authored nodes and prove author provenance.

```bash
cargo run --bin eontree-cli -- keygen --output my_key

```

* **Output:** Creates `my_key.pub` (public key) and `my_key.priv` (private key).

---

## 2. Author and Sign a Decision Node (`create`)

Drafts a new `DecisionNode`, automatically computes its unique content-addressable Blake3 hash (`NodeId`), attaches evidence references, and serializes the signed payload to disk.

```bash
cargo run --bin eontree-cli -- create \
  --title "Global Phosphorus Depletion" \
  --content "Reserves are projected to peak and decline by 2040, threatening agricultural yields." \
  --node-type "problem" \
  --parent-id "root_axiom_placeholder_hash" \
  --key-file "my_key.priv" \
  --output "proposal.json"

```

* **Supported Node Types:** `root`, `problem`, `unknown`, `barrier`, `innovation`.
* **Output:** Saves the fully structured node JSON to `proposal.json`.

---

## 3. Submit to the API Server (`submit`)

Reads a signed node JSON file and dispatches a `POST /nodes` request to your local or remote `eontree-api` server. The server will run the Global Sentinel ethical pre-filter and the weighted Triple-LLM jury audit before committing the record.

```bash
cargo run --bin eontree-cli -- submit \
  --input "proposal.json" \
  --endpoint "http://127.0.0.1:8080"

```

* **Success:** Returns `201 Created` upon passing all validation checks and being written to the persistent `redb` store.
* **Rejection:** Returns `422 Unprocessable Entity` if flagged by the Sentinel, or if the weighted jury score falls below the node's passing threshold (the response body includes the `NodeScorecard` for review).

---

## 4. List the Opposition Todo Queue (`list-todos`)

Fetches every node currently awaiting community critique (`PendingOpposition` status) from a running `eontree-api` server: the adversarial objections, failure modes, and competing alternatives automatically spawned alongside each approved `Innovation`/`Problem` node.

```bash
cargo run --bin eontree-cli -- list-todos --endpoint "http://127.0.0.1:8080"

```

* **Output:** Prints each stub's node type, title, ID, and parent ID.

---

## 5. Claim a Todo Stub (`claim`)

Fetches a `PendingOpposition` stub by ID, lets you supply real argumentation and evidence, signs a new node with your own key (reusing the stub's parent and taxonomic type), and submits it to the server.

```bash
cargo run --bin eontree-cli -- claim --todo-id "stub_node_id_hash" --content "A rigorous, evidence-backed counter-argument fleshing out the stub." --evidence "doi:10.1016/example, https://example.com/report" --key-file "my_key.priv" --endpoint "http://127.0.0.1:8080" --output "claimed_node.json"

```

* **Behavior:** The claimed node is a brand-new, independently signed `DecisionNode` (content-addressable IDs can't be reused across different authors/content); the original stub remains in the graph as a historical `PendingOpposition` record.
* **Success:** Returns `201 Created` upon passing all validation checks, with the new node saved locally to `claimed_node.json`.
* **Rejection:** Same failure modes as `submit` (invalid signature, Sentinel/taxonomy violation, or jury score below threshold).
