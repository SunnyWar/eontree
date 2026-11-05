---

# eontree-jury

`eontree-jury` is the automated verification, ethical filtering, and synthetic peer-review engine for **EonTree**. It acts as the gatekeeper for the decision tree, ensuring that no proposed node or solution pathway enters the graph without passing rigorous ethical constraints and distributed multi-model consensus.

---

## Core Architecture

To balance absolute platform openness with uncompromising academic and ethical rigor, `eontree-jury` implements a two-stage evaluation pipeline:

1. **The Global Sentinel Protocol (Ethical Pre-Filter):** A fast-path security and meta-constraint layer that intercepts proposals before compute is spent on full LLM audits.
2. **The Triple-LLM Jury (Parallel Consensus Auditor):** An asynchronous asynchronous audit pipeline that dispatches evaluation prompts to three independent LLM endpoints in parallel, enforcing a strict $2/3$ majority agreement.

---

## 1. The Global Sentinel Protocol (`sentinel.rs`)

The Sentinel evaluates every incoming `DecisionNode` against EonTree's core non-anthropocentric axiom: *does this solution achieve progress by offloading suffering onto the non-human world or permanently degrading the biosphere?*

* **Fast Rejection:** If a node's title or content matches restricted ecological hazard patterns or unmitigated biosphere degradation vectors, the Sentinel immediately rejects the proposal with a `SentinelError::ConstraintViolation`.
* **Extensibility:** Designed to integrate specialized safety classifiers or policy heuristics before a proposal reaches the jury.

---

## 2. The Triple-LLM Jury & Consensus Engine (`jury.rs`)

Once a node passes the Sentinel pre-filter, it enters the consensus pipeline managed by `LLMJudgeClient`:

* **Parallel Asynchronous Dispatch:** Utilizes `tokio` and `reqwest` to dispatch evaluation payloads simultaneously across three independent judge endpoints (`Judge-Alpha`, `Judge-Beta`, `Judge-Gamma`) using `tokio::join!`.
* **Structured Scorecards:** Each judge returns a rigorous JSON scorecard evaluating the proposal across logical validity, clarity, and evidence backing.
* **The 2-of-3 Rule:** A reducer aggregates the scorecard votes. If fewer than 2 out of 3 judges vote `approved: true`, the consensus engine rejects the commit and returns a `JuryError::ConsensusFailed`.

---

## Data Structures

### `JudgeScorecard`

```rust
pub struct JudgeScorecard {
    pub judge_id: String,       // e.g., "Judge-Alpha"
    pub approved: bool,         // Binary consensus vote
    pub logic_score: u8,        // 1-10 logical rigor rating
    pub clarity_score: u8,      // 1-10 prose and structuring rating
    pub critique: String,       // Detailed qualitative feedback
}

```

---

## Usage Example

```rust
use eontree_core::DecisionNode;
use eontree_jury::{GlobalSentinel, LLMJudgeClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node: DecisionNode = /* ... load or receive proposed node ... */;

    // 1. Run Global Sentinel ethical pre-filter
    GlobalSentinel::evaluate(&node)?;

    // 2. Initialize Triple-LLM jury client with three independent endpoints
    let jury = LLMJudgeClient::new([
        "https://api.judge-alpha.eontree.org/v1/eval".into(),
        "https://api.judge-beta.eontree.org/v1/eval".into(),
        "https://api.judge-gamma.eontree.org/v1/eval".into(),
    ]);

    // 3. Execute parallel audit and verify 2/3 consensus
    let scorecards = jury.audit_node(&node).await?;
    
    println!("Consensus achieved! Received {} valid scorecards.", scorecards.len());
    Ok(())
}

```