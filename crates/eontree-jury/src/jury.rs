use eontree_core::DecisionNode;
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
    #[error("Consensus failed: only {approved_count}/3 judges approved (Required: >= 2)")]
    ConsensusFailed { approved_count: usize },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JudgeScorecard {
    pub judge_id: String,
    pub approved: bool,
    pub logic_score: u8, // 1-10
    pub clarity_score: u8,
    pub critique: String,
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

    /// Audits a node in parallel across all three independent LLM judges.
    pub async fn audit_node(&self, node: &DecisionNode) -> Result<Vec<JudgeScorecard>, JuryError> {
        let fut1 = self.query_judge(&self.endpoint_urls[0], "Judge-Alpha", node);
        let fut2 = self.query_judge(&self.endpoint_urls[1], "Judge-Beta", node);
        let fut3 = self.query_judge(&self.endpoint_urls[2], "Judge-Gamma", node);

        let (res1, res2, res3) = tokio::join!(fut1, fut2, fut3);

        let scorecards = vec![res1?, res2?, res3?];

        let approved_count = scorecards.iter().filter(|s| s.approved).count();
        if approved_count < 2 {
            return Err(JuryError::ConsensusFailed { approved_count });
        }

        Ok(scorecards)
    }

    async fn query_judge(
        &self,
        endpoint: &str,
        judge_id: &str,
        node: &DecisionNode,
    ) -> Result<JudgeScorecard, JuryError> {
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
        // let resp = self.client.post(endpoint).json(&payload).send().await?.json::<JudgeScorecard>().await?;
        // Ok(resp)

        // Mock return for compilation scaffolding:
        Ok(JudgeScorecard {
            judge_id: judge_id.to_string(),
            approved: true,
            logic_score: 9,
            clarity_score: 10,
            critique: "Sound first-principles reasoning with verifiable evidence framing.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eontree_core::NodeType;

    fn sample_node() -> DecisionNode {
        DecisionNode::new(
            None,
            NodeType::Innovation,
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

    #[tokio::test]
    async fn audit_node_returns_a_scorecard_per_judge() {
        let node = sample_node();

        let scorecards = client()
            .audit_node(&node)
            .await
            .expect("mock judges should approve");

        assert_eq!(scorecards.len(), 3);
        assert!(scorecards.iter().all(|s| s.approved));
        assert_eq!(scorecards[0].judge_id, "Judge-Alpha");
        assert_eq!(scorecards[1].judge_id, "Judge-Beta");
        assert_eq!(scorecards[2].judge_id, "Judge-Gamma");
    }

    #[test]
    fn consensus_failed_error_formats_approved_count() {
        let err = JuryError::ConsensusFailed { approved_count: 1 };
        assert_eq!(
            err.to_string(),
            "Consensus failed: only 1/3 judges approved (Required: >= 2)"
        );
    }
}
