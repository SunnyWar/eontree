use eontree_core::DecisionNode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SentinelError {
    #[error(
        "Global Sentinel violation: Proposed node triggers biosphere degradation or non-human suffering constraint."
    )]
    ConstraintViolation(String),
}

pub struct GlobalSentinel;

impl GlobalSentinel {
    /// Evaluates a node against ethical meta-constraints.
    pub fn evaluate(node: &DecisionNode) -> Result<(), SentinelError> {
        let content_lower = node.content.to_lowercase();
        let title_lower = node.title.to_lowercase();

        // Example baseline keyword and pattern checks for biosphere harm
        // In production, this can be backed by a specialized safety classifier.
        let hazardous_patterns = [
            "permanently degrade the biosphere",
            "offload suffering to ecosystems",
            "toxic waste dumping",
            "unmitigated geoengineering hazard",
        ];

        for pattern in &hazardous_patterns {
            if content_lower.contains(pattern) || title_lower.contains(pattern) {
                return Err(SentinelError::ConstraintViolation(format!(
                    "Matched restricted ecological hazard pattern: '{}'",
                    pattern
                )));
            }
        }

        Ok(())
    }
}
