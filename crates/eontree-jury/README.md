---

# eontree-jury

`eontree-jury` is the automated verification, ethical filtering, and synthetic peer-review engine for **EonTree**. It acts as the gatekeeper for the decision tree, ensuring that no proposed node or solution pathway enters the graph without passing rigorous ethical constraints and distributed multi-model consensus.

---

## Core Architecture

To balance absolute platform openness with uncompromising academic and ethical rigor, `eontree-jury` implements a two-stage evaluation pipeline:

1. **The Global Sentinel Protocol (Ethical Pre-Filter):** A fast-path security and meta-constraint layer that intercepts proposals before compute is spent on full LLM audits. This is a hard invariant: strictly binary pass/fail.
2. **The Weighted Epistemic Jury (Graded-Merit Auditor):** An asynchronous audit pipeline that dispatches evaluation prompts to three independent LLM endpoints in parallel and folds their opinions into a single weighted `NodeScorecard`, replacing rigid binary pass/fail judging with graded-merit scoring.

---

## 1. The Global Sentinel Protocol (`sentinel.rs`)

The Sentinel evaluates every incoming `DecisionNode` against EonTree's core non-anthropocentric axiom: *does this solution achieve progress by offloading suffering onto the non-human world or permanently degrading the biosphere?*

* **Fast Rejection:** If a node's title or content matches restricted ecological hazard patterns or unmitigated biosphere degradation vectors, the Sentinel immediately rejects the proposal with a `SentinelError::ConstraintViolation`.
* **Extensibility:** Designed to integrate specialized safety classifiers or policy heuristics before a proposal reaches the jury.

---

## 2. The Weighted Epistemic Jury & Scoring Engine (`jury.rs`)

Once a node passes the Sentinel pre-filter, it enters the graded-merit evaluation pipeline managed by `LLMJudgeClient`:

* **Parallel Asynchronous Dispatch:** Utilizes `tokio` and `reqwest` to dispatch evaluation payloads simultaneously across three independent judge endpoints (`Judge-Alpha`, `Judge-Beta`, `Judge-Gamma`) using `tokio::join!`.
* **Graded Merit Scoring:** Each judge contributes an equally-weighted `EvaluationCriterion` (derived from its logic and clarity ratings) instead of casting a binary approve/reject vote.
* **Composite Threshold Check:** `LLMJudgeClient::evaluate_node_weighted` folds the three criteria into a single `NodeScorecard`. A node is approved only if its weighted composite score meets or exceeds `passing_threshold`, which scales with node criticality: `Root`/`Barrier` nodes require `0.9`, `Problem`/`Innovation` require `0.75`, and exploratory `Unknown` nodes require only `0.6`.

---

## Data Structures

### `EvaluationCriterion`

```rust
pub struct EvaluationCriterion {
    pub name: String,       // e.g., "Judge-Alpha"
    pub weight: f64,        // Contribution weight (weights across criteria sum to 1.0)
    pub score: f64,         // Normalized score between 0.0 and 1.0
    pub rationale: String,  // Qualitative justification
}
```

### `NodeScorecard`

```rust
pub struct NodeScorecard {
    pub criteria: Vec<EvaluationCriterion>,
    pub passing_threshold: f64,
}

impl NodeScorecard {
    pub fn add_criterion(&mut self, name: &str, weight: f64, score: f64, rationale: &str);
    pub fn composite_score(&self) -> f64; // weighted sum of all criteria
    pub fn is_approved(&self) -> bool;    // composite_score() >= passing_threshold
}
```

---

## Usage Example

```rust
use eontree_core::DecisionNode;
use eontree_jury::LLMJudgeClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node: DecisionNode = /* ... load or receive proposed node ... */;

    // 1. Run the Global Sentinel ethical pre-filter first (hard invariant, binary).
    // GlobalSentinel::evaluate(&store, &root_axiom_id, &node)?;

    // 2. Initialize the jury client with three independent endpoints
    let jury = LLMJudgeClient::new([
        "https://api.judge-alpha.eontree.org/v1/eval".into(),
        "https://api.judge-beta.eontree.org/v1/eval".into(),
        "https://api.judge-gamma.eontree.org/v1/eval".into(),
    ]);

    // 3. Execute the parallel audit and collect a weighted scorecard
    let scorecard = jury.evaluate_node_weighted(&node).await?;

    if scorecard.is_approved() {
        println!("Approved! Composite score: {:.2}", scorecard.composite_score());
    } else {
        println!(
            "Rejected. Composite score {:.2} is below threshold {:.2}",
            scorecard.composite_score(),
            scorecard.passing_threshold
        );
    }

    Ok(())
}
```
