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

    pub fn validate_taxonomy(
        parent_node: &DecisionNode,
        child_node: &DecisionNode,
    ) -> Result<(), &'static str> {
        match (parent_node.node_type, child_node.node_type) {
            // Allow recursive problem decomposition (Problem broken into Sub-Problems)
            (NodeType::Problem, NodeType::Problem) => Ok(()),

            // Allow standard downward flows
            (NodeType::Root, NodeType::Problem) => Ok(()),
            (NodeType::Problem, NodeType::Unknown) | (NodeType::Problem, NodeType::Barrier) => {
                Ok(())
            }
            (NodeType::Barrier, NodeType::Innovation) => Ok(()),
            (NodeType::Unknown, NodeType::Innovation) => Ok(()),

            _ => Err("Invalid taxonomic transition in decision tree hierarchy"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeStore {
        children: Vec<DecisionNode>,
        fail: bool,
    }

    impl GraphStore for FakeStore {
        type Error = String;

        fn get_node(&self, _id: &NodeId) -> Result<Option<DecisionNode>, Self::Error> {
            unimplemented!("not exercised by GlobalSentinel::evaluate")
        }

        fn put_node(&mut self, _node: &DecisionNode) -> Result<(), Self::Error> {
            unimplemented!("not exercised by GlobalSentinel::evaluate")
        }

        fn get_children(&self, _id: &NodeId) -> Result<Vec<DecisionNode>, Self::Error> {
            if self.fail {
                Err("store unavailable".to_string())
            } else {
                Ok(self.children.clone())
            }
        }

        fn list_by_status(
            &self,
            _status: eontree_core::NodeStatus,
        ) -> Result<Vec<DecisionNode>, Self::Error> {
            unimplemented!("not exercised by GlobalSentinel::evaluate")
        }
    }

    fn constraint(node_type: NodeType, title: &str, content: &str) -> DecisionNode {
        DecisionNode::new(
            None,
            node_type,
            title.into(),
            content.into(),
            vec![],
            "author".into(),
        )
    }

    fn candidate(title: &str, content: &str) -> DecisionNode {
        DecisionNode::new(
            None,
            NodeType::Innovation,
            title.into(),
            content.into(),
            vec![],
            "author".into(),
        )
    }

    #[test]
    fn evaluate_allows_node_with_no_constraints() {
        let store = FakeStore {
            children: vec![],
            fail: false,
        };
        let root = NodeId("root".into());
        let node = candidate("New idea", "Something harmless");

        assert!(GlobalSentinel::evaluate(&store, &root, &node).is_ok());
    }

    #[test]
    fn evaluate_rejects_node_whose_content_matches_problem_constraint() {
        let store = FakeStore {
            children: vec![constraint(
                NodeType::Problem,
                "Banned topic",
                "forbidden phrase",
            )],
            fail: false,
        };
        let root = NodeId("root".into());
        let node = candidate("Some title", "This contains a forbidden phrase indeed");

        let err = GlobalSentinel::evaluate(&store, &root, &node).unwrap_err();
        assert!(matches!(err, SentinelError::ConstraintViolation(_)));
    }

    #[test]
    fn evaluate_rejects_node_whose_title_matches_root_constraint() {
        let store = FakeStore {
            children: vec![constraint(NodeType::Root, "root constraint", "blockme")],
            fail: false,
        };
        let root = NodeId("root".into());
        let node = candidate("this should blockme please", "irrelevant content");

        assert!(GlobalSentinel::evaluate(&store, &root, &node).is_err());
    }

    #[test]
    fn evaluate_ignores_constraints_of_other_node_types() {
        let store = FakeStore {
            children: vec![constraint(NodeType::Barrier, "irrelevant", "forbidden")],
            fail: false,
        };
        let root = NodeId("root".into());
        let node = candidate(
            "Some title",
            "this contains forbidden word but type is Barrier",
        );

        assert!(GlobalSentinel::evaluate(&store, &root, &node).is_ok());
    }

    #[test]
    fn evaluate_propagates_store_errors() {
        let store = FakeStore {
            children: vec![],
            fail: true,
        };
        let root = NodeId("root".into());
        let node = candidate("title", "content");

        let err = GlobalSentinel::evaluate(&store, &root, &node).unwrap_err();
        assert!(matches!(err, SentinelError::Store(_)));
    }
}
