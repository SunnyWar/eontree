use eontree_core::{DecisionNode, NodeType};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JuryError {
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

// A six-dimensional epistemic quality signal for a node. Each dimension is a raw
// score intended to fall within [0.0, 1.0]; composite_score clamps defensively in
// case an upstream judge returns an out-of-range value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub plausibility: f32,
    pub evidence_strength: f32,
    pub predictive_power: f32,
    pub novelty: f32,
    pub explanatory_power: f32,
    pub solution_relevance: f32,
}

impl NodeMetrics {
    fn weighted_pairs(&self, weights: &MetricWeights) -> [(f32, f32); 6] {
        [
            (self.plausibility, weights.plausibility),
            (self.evidence_strength, weights.evidence_strength),
            (self.predictive_power, weights.predictive_power),
            (self.novelty, weights.novelty),
            (self.explanatory_power, weights.explanatory_power),
            (self.solution_relevance, weights.solution_relevance),
        ]
    }
}

fn average_metrics(samples: &[NodeMetrics]) -> NodeMetrics {
    let n = samples.len() as f32;
    NodeMetrics {
        plausibility: samples.iter().map(|m| m.plausibility).sum::<f32>() / n,
        evidence_strength: samples.iter().map(|m| m.evidence_strength).sum::<f32>() / n,
        predictive_power: samples.iter().map(|m| m.predictive_power).sum::<f32>() / n,
        novelty: samples.iter().map(|m| m.novelty).sum::<f32>() / n,
        explanatory_power: samples.iter().map(|m| m.explanatory_power).sum::<f32>() / n,
        solution_relevance: samples.iter().map(|m| m.solution_relevance).sum::<f32>() / n,
    }
}

// Per-dimension contribution weights used when folding NodeMetrics into a single
// composite score. The Default baseline sums to 1.0, but composite_score
// normalizes against the actual total so custom weightings need not sum exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricWeights {
    pub plausibility: f32,
    pub evidence_strength: f32,
    pub predictive_power: f32,
    pub novelty: f32,
    pub explanatory_power: f32,
    pub solution_relevance: f32,
}

impl Default for MetricWeights {
    fn default() -> Self {
        Self {
            plausibility: 0.25,
            evidence_strength: 0.25,
            predictive_power: 0.15,
            novelty: 0.1,
            explanatory_power: 0.15,
            solution_relevance: 0.1,
        }
    }
}

// A graded-merit evaluation of a node, replacing rigid binary pass/fail judging.
// composite_score is the weight-normalized sum of all six metric dimensions; a
// node is approved only if that composite meets or exceeds passing_threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScorecard {
    pub metrics: NodeMetrics,
    pub weights: MetricWeights,
    pub passing_threshold: f32,
    pub rationales: HashMap<String, String>,
}

impl NodeScorecard {
    pub fn new(metrics: NodeMetrics, weights: MetricWeights, passing_threshold: f32) -> Self {
        Self {
            metrics,
            weights,
            passing_threshold,
            rationales: HashMap::new(),
        }
    }

    pub fn add_rationale(&mut self, dimension: &str, rationale: &str) {
        self.rationales
            .insert(dimension.to_string(), rationale.to_string());
    }

    pub fn composite_score(&self) -> f32 {
        let pairs = self.metrics.weighted_pairs(&self.weights);
        let total_weight: f32 = pairs.iter().map(|(_, w)| w).sum();
        if total_weight <= 0.0 {
            return 0.0;
        }

        let weighted_sum: f32 = pairs
            .iter()
            .map(|(score, w)| score.clamp(0.0, 1.0) * w)
            .sum();
        weighted_sum / total_weight
    }

    pub fn is_approved(&self) -> bool {
        self.composite_score() >= self.passing_threshold
    }
}

// The minimum composite score required for a node to be approved, scaled by how
// foundational or safety-critical its type is: axioms and barriers are held to a
// higher bar than exploratory Unknown nodes.
fn default_passing_threshold(node_type: NodeType) -> f32 {
    match node_type {
        NodeType::Root | NodeType::Barrier => 0.9,
        NodeType::Problem | NodeType::Innovation => 0.75,
        NodeType::Unknown => 0.6,
    }
}

// Raw per-judge opinion prior to being folded into a NodeScorecard.
#[derive(Debug, Serialize, Deserialize)]
struct JudgeOpinion {
    judge_id: String,
    metrics: NodeMetrics,
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

    // Audits a node in parallel across all three independent LLM judges, averages
    // their six-dimensional metrics into a single NodeMetrics reading, and folds
    // the result into a NodeScorecard using the default metric weights and a
    // passing threshold scaled by the node type.
    pub async fn evaluate_node_weighted(
        &self,
        node: &DecisionNode,
    ) -> Result<NodeScorecard, JuryError> {
        let fut1 = self.query_judge(&self.endpoint_urls[0], "Judge-Alpha", node);
        let fut2 = self.query_judge(&self.endpoint_urls[1], "Judge-Beta", node);
        let fut3 = self.query_judge(&self.endpoint_urls[2], "Judge-Gamma", node);

        let (res1, res2, res3) = tokio::join!(fut1, fut2, fut3);
        let opinions = [res1?, res2?, res3?];

        let metrics = average_metrics(&[
            opinions[0].metrics,
            opinions[1].metrics,
            opinions[2].metrics,
        ]);

        let mut scorecard = NodeScorecard::new(
            metrics,
            MetricWeights::default(),
            default_passing_threshold(node.node_type),
        );
        for opinion in &opinions {
            scorecard.add_rationale(&opinion.judge_id, &opinion.critique);
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
                    "content": "You are a rigorous synthetic peer-review juror for EonTree. Evaluate the plausibility, evidence strength, predictive power, novelty, explanatory power, and solution relevance of the proposed node."
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
            metrics: NodeMetrics {
                plausibility: 0.9,
                evidence_strength: 0.85,
                predictive_power: 0.8,
                novelty: 0.7,
                explanatory_power: 0.9,
                solution_relevance: 0.85,
            },
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

    fn uniform_metrics(value: f32) -> NodeMetrics {
        NodeMetrics {
            plausibility: value,
            evidence_strength: value,
            predictive_power: value,
            novelty: value,
            explanatory_power: value,
            solution_relevance: value,
        }
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
    fn default_metric_weights_sum_to_one() {
        let w = MetricWeights::default();
        let total = w.plausibility
            + w.evidence_strength
            + w.predictive_power
            + w.novelty
            + w.explanatory_power
            + w.solution_relevance;
        assert!((total - 1.0).abs() < 1e-6);
    }

    #[test]
    fn composite_score_is_weighted_average_across_all_dimensions() {
        let metrics = NodeMetrics {
            plausibility: 1.0,
            evidence_strength: 0.5,
            predictive_power: 1.0,
            novelty: 0.0,
            explanatory_power: 1.0,
            solution_relevance: 0.5,
        };
        let weights = MetricWeights::default();
        let scorecard = NodeScorecard::new(metrics, weights.clone(), 0.75);

        let expected = (metrics.plausibility * weights.plausibility
            + metrics.evidence_strength * weights.evidence_strength
            + metrics.predictive_power * weights.predictive_power
            + metrics.novelty * weights.novelty
            + metrics.explanatory_power * weights.explanatory_power
            + metrics.solution_relevance * weights.solution_relevance)
            / 1.0;

        assert!((scorecard.composite_score() - expected).abs() < 1e-6);
    }

    #[test]
    fn composite_score_clamps_out_of_range_metrics() {
        let metrics = NodeMetrics {
            plausibility: 1.5,
            evidence_strength: -0.5,
            predictive_power: 0.5,
            novelty: 0.5,
            explanatory_power: 0.5,
            solution_relevance: 0.5,
        };
        let scorecard = NodeScorecard::new(metrics, MetricWeights::default(), 0.5);

        // All inputs clamp into [0.0, 1.0], so the composite must land there too.
        let score = scorecard.composite_score();
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn is_approved_respects_passing_threshold() {
        let mut scorecard = NodeScorecard::new(uniform_metrics(0.5), MetricWeights::default(), 0.8);
        assert!(!scorecard.is_approved());

        scorecard.metrics = uniform_metrics(0.95);
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
    async fn evaluate_node_weighted_returns_populated_scorecard() {
        let node = sample_node(NodeType::Innovation);

        let scorecard = client()
            .evaluate_node_weighted(&node)
            .await
            .expect("mock judges should respond");

        assert_eq!(scorecard.rationales.len(), 3);
        assert!(scorecard.rationales.contains_key("Judge-Alpha"));
        assert!(scorecard.rationales.contains_key("Judge-Beta"));
        assert!(scorecard.rationales.contains_key("Judge-Gamma"));
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
