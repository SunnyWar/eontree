use eontree_core::{DecisionNode, GraphStore, NodeId, NodeType};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SentinelError {
    #[error("Global Sentinel violation: {0}")]
    ConstraintViolation(String),
    #[error("Storage error during sentinel evaluation: {0}")]
    Store(String),
}

pub struct GlobalSentinel;

impl GlobalSentinel {
    /// Evaluates a node dynamically by traversing constraint nodes rooted at a known axiom ID.
    pub fn evaluate<S: GraphStore>(
        store: &S,
        root_axiom_id: &NodeId,
        node: &DecisionNode,
    ) -> Result<(), SentinelError> {
        // Fetch active constraints using string conversion for the store error
        let constraint_nodes = store
            .get_children(root_axiom_id)
            .map_err(|e| SentinelError::Store(e.to_string()))?;

        let content_lower = node.content.to_lowercase();
        let title_lower = node.title.to_lowercase();

        for constraint in constraint_nodes {
            if constraint.node_type == NodeType::Problem || constraint.node_type == NodeType::Root {
                let pattern = constraint.content.to_lowercase();
                if content_lower.contains(&pattern) || title_lower.contains(&pattern) {
                    return Err(SentinelError::ConstraintViolation(format!(
                        "Matched graph-defined constraint [{}]: {}",
                        constraint.title, constraint.content
                    )));
                }
            }
        }

        Ok(())
    }
}
