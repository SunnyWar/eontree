use eontree_core::{DecisionNode, NodeType};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JuryError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

// A single weighted dimension of a graded epistemic evaluation for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCriterion {
    pub name: String,
    pub weight: f64,
    pub score: f64,
    pub rationale: String,
}

// A graded-merit evaluation of a node, replacing rigid binary pass/fail judging.
// composite_score is the weight-normalized sum of all criteria; a node is
// approved only if that composite meets or exceeds passing_threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScorecard {
    pub criteria: Vec<EvaluationCriterion>,
    pub passing_threshold: f64,
}

impl NodeScorecard {
    pub fn new(passing_threshold: f64) -> Self {
        Self {
            criteria: Vec::new(),
            passing_threshold,
        }
    }

    pub fn add_criterion(&mut self, name: &str, weight: f64, score: f64, rationale: &str) {
        self.criteria.push(EvaluationCriterion {
            name: name.to_string(),
            weight,
            score,
            rationale: rationale.to_string(),
        });
    }

    pub fn composite_score(&self) -> f64 {
        self.criteria.iter().map(|c| c.weight * c.score).sum()
    }

    pub fn is_approved(&self) -> bool {
        self.composite_score() >= self.passing_threshold
    }
}

// The minimum composite score required for a node to be approved, scaled by how
// foundational or safety-critical its type is: axioms and barriers are held to a
// higher bar than exploratory Unknown nodes.
fn default_passing_threshold(node_type: NodeType) -> f64 {
    match node_type {
        NodeType::Root | NodeType::Barrier => 0.9,
        NodeType::Problem | NodeType::Innovation => 0.75,
        NodeType::Unknown => 0.6,
    }
}

// Raw per-judge opinion prior to being folded into a weighted NodeScorecard.
#[derive(Debug, Serialize, Deserialize)]
struct JudgeOpinion {
    judge_id: String,
    logic_score: u8, // 1-10
    clarity_score: u8,
    critique: String,
}

pub struct LLMJudgeClient {
    client: Client,
    endpoint_urls: [String; 3],
}

impl LLMJudgeClient {
    pub fn new(endpoints: [String; 3]) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        Self {
            client,
            endpoint_urls: endpoints,
        }
    }

    // Audits a node in parallel across all three independent LLM judges and folds
    // their opinions into a single weighted NodeScorecard. Each judge contributes
    // an equally-weighted criterion derived from the average of its logic and
    // clarity scores; the passing threshold is scaled by the node type.
    pub async fn evaluate_node_weighted(
        &self,
        node: &DecisionNode,
    ) -> Result<NodeScorecard, JuryError> {
        let fut1 = self.query_judge(&self.endpoint_urls[0], "Judge-Alpha", node);
        let fut2 = self.query_judge(&self.endpoint_urls[1], "Judge-Beta", node);
        let fut3 = self.query_judge(&self.endpoint_urls[2], "Judge-Gamma", node);

        let (res1, res2, res3) = tokio::join!(fut1, fut2, fut3);
        let opinions = [res1?, res2?, res3?];

        let mut scorecard = NodeScorecard::new(default_passing_threshold(node.node_type));
        let weight = 1.0 / opinions.len() as f64;
        for opinion in &opinions {
            let score = (opinion.logic_score as f64 + opinion.clarity_score as f64) / 20.0;
            scorecard.add_criterion(&opinion.judge_id, weight, score, &opinion.critique);
        }

        Ok(scorecard)
    }
    async fn query_judge(
        &self,
        endpoint: &str,
        judge_id: &str,
        node: &DecisionNode,
    ) -> Result<JudgeOpinion, JuryError> {
        // Construct prompt payload for the LLM judge
        let payload = serde_json::json!({
            "model": "eontree-validator-v1",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a rigorous synthetic peer-review juror for EonTree. Evaluate the logical validity, evidence backing, and clarity of the proposed node."
                },
                {
                    "role": "user",
                    "content": format!("Evaluate this node:\nTitle: {}\nContent: {}\nType: {:?}", node.title, node.content, node.node_type)
                }
            ]
        });

        // Uncomment when wiring up to active local/remote inference endpoints:
        // let resp = self.client.post(endpoint).json(&payload).send().await?.json::<JudgeOpinion>().await?;
        // Ok(resp)

        // Mock return for compilation scaffolding:
        Ok(JudgeOpinion {
            judge_id: judge_id.to_string(),
            logic_score: 9,
            clarity_score: 10,
            critique: "Sound first-principles reasoning with verifiable evidence framing.".into(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node(node_type: NodeType) -> DecisionNode {
        DecisionNode::new(
            None,
            node_type,
            "Title".into(),
            "Content".into(),
            vec![],
            "author".into(),
        )
    }

    fn client() -> LLMJudgeClient {
        LLMJudgeClient::new([
            "http://judge-alpha.invalid".into(),
            "http://judge-beta.invalid".into(),
            "http://judge-gamma.invalid".into(),
        ])
    }

    #[test]
    fn new_stores_all_three_endpoints() {
        let client = client();
        assert_eq!(
            client.endpoint_urls,
            [
                "http://judge-alpha.invalid".to_string(),
                "http://judge-beta.invalid".to_string(),
                "http://judge-gamma.invalid".to_string(),
            ]
        );
    }

    #[test]
    fn composite_score_is_weighted_sum() {
        let mut scorecard = NodeScorecard::new(0.75);
        scorecard.add_criterion("Logic", 0.6, 0.9, "Sound reasoning");
        scorecard.add_criterion("Evidence", 0.4, 0.5, "Weak citations");

        let expected = 0.6 * 0.9 + 0.4 * 0.5;
        assert!((scorecard.composite_score() - expected).abs() < 1e-9);
    }

    #[test]
    fn is_approved_respects_passing_threshold() {
        let mut scorecard = NodeScorecard::new(0.8);
        scorecard.add_criterion("Only", 1.0, 0.79, "Just short");
        assert!(!scorecard.is_approved());

        scorecard.criteria[0].score = 0.8;
        assert!(scorecard.is_approved());
    }

    #[test]
    fn passing_threshold_scales_with_node_criticality() {
        assert!(
            default_passing_threshold(NodeType::Root)
                > default_passing_threshold(NodeType::Problem)
        );
        assert!(
            default_passing_threshold(NodeType::Barrier)
                > default_passing_threshold(NodeType::Innovation)
        );
        assert!(
            default_passing_threshold(NodeType::Unknown)
                < default_passing_threshold(NodeType::Problem)
        );
    }

    #[tokio::test]
    async fn evaluate_node_weighted_returns_scorecard_with_one_criterion_per_judge() {
        let node = sample_node(NodeType::Innovation);

        let scorecard = client()
            .evaluate_node_weighted(&node)
            .await
            .expect("mock judges should respond");

        assert_eq!(scorecard.criteria.len(), 3);
        assert_eq!(scorecard.criteria[0].name, "Judge-Alpha");
        assert_eq!(scorecard.criteria[1].name, "Judge-Beta");
        assert_eq!(scorecard.criteria[2].name, "Judge-Gamma");
        assert!((scorecard.criteria.iter().map(|c| c.weight).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(
            scorecard.passing_threshold,
            default_passing_threshold(NodeType::Innovation)
        );
        assert!(scorecard.is_approved());
    }

    #[tokio::test]
    async fn evaluate_node_weighted_uses_higher_threshold_for_root_nodes() {
        let node = sample_node(NodeType::Root);

        let scorecard = client()
            .evaluate_node_weighted(&node)
            .await
            .expect("mock judges should respond");

        assert_eq!(scorecard.passing_threshold, 0.9);
    }
}
