use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use eontree_core::{DecisionNode, GraphStore, NodeId, store::RedbStore};
use eontree_jury::{GlobalSentinel, LLMJudgeClient, NodeScorecard};
use std::sync::Arc;
use tokio::sync::Mutex;

struct AppState {
    store: Mutex<RedbStore>,
    jury: LLMJudgeClient,
    root_axiom_id: NodeId,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize persistent storage
    let store = RedbStore::open("eontree.db")?;

    let state = Arc::new(AppState {
        store: Mutex::new(store),
        jury: LLMJudgeClient::new([
            "https://api.judge-alpha.eontree.org/v1/eval".into(),
            "https://api.judge-beta.eontree.org/v1/eval".into(),
            "https://api.judge-gamma.eontree.org/v1/eval".into(),
        ]),
        root_axiom_id: NodeId("root_axiom_placeholder_hash".into()),
    });

    // 2. Build Axum router
    let app = Router::new()
        .route("/nodes/:id", get(get_node_handler))
        .route("/nodes/:id/children", get(get_children_handler))
        .route("/nodes", post(post_node_handler))
        .with_state(state);

    // 3. Bind and listen
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    println!("EonTree API server listening on http://127.0.0.1:8080");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_node_handler(
    State(state): State<Arc<AppState>>,
    Path(id_str): Path<String>,
) -> Result<Json<DecisionNode>, StatusCode> {
    let store = state.store.lock().await;
    let node_id = NodeId(id_str);

    match store.get_node(&node_id) {
        Ok(Some(node)) => Ok(Json(node)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_children_handler(
    State(state): State<Arc<AppState>>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<DecisionNode>>, StatusCode> {
    let store = state.store.lock().await;
    let node_id = NodeId(id_str);

    match store.get_children(&node_id) {
        Ok(children) => Ok(Json(children)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// Rejection outcomes for node ingestion. Signature and Sentinel failures remain
// strictly binary hard invariants; JuryRejected carries the graded scorecard that
// produced the rejection so callers can audit exactly which criteria fell short.
#[derive(Debug)]
enum PostNodeError {
    InvalidSignature,
    PolicyViolation,
    JuryRejected(NodeScorecard),
    Internal,
    ParentNotFound,
    InvalidTaxonomy,
}

impl IntoResponse for PostNodeError {
    fn into_response(self) -> Response {
        match self {
            PostNodeError::InvalidSignature => StatusCode::BAD_REQUEST.into_response(),
            PostNodeError::PolicyViolation => StatusCode::UNPROCESSABLE_ENTITY.into_response(),
            PostNodeError::JuryRejected(scorecard) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(scorecard)).into_response()
            }
            PostNodeError::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            PostNodeError::ParentNotFound => StatusCode::NOT_FOUND.into_response(),
            PostNodeError::InvalidTaxonomy => StatusCode::UNPROCESSABLE_ENTITY.into_response(),
        }
    }
}

async fn post_node_handler(
    State(state): State<Arc<AppState>>,
    Json(node): Json<DecisionNode>,
) -> Result<StatusCode, PostNodeError> {
    // Hard invariant 1: cryptographic signature must be valid.
    if node.verify().is_err() {
        return Err(PostNodeError::InvalidSignature);
    }

    let mut store = state.store.lock().await;

    // Hard invariant 2: Structural Parent & Taxonomic Validation
    if let Some(ref parent_id) = node.parent_id {
        let parent_node = store
            .get_node(parent_id)
            .map_err(|_| PostNodeError::Internal)?
            .ok_or(PostNodeError::ParentNotFound)?; // Ensure parent exists

        // Validate taxonomic transition (permits Problem -> Problem decomposition)
        if GlobalSentinel::validate_taxonomy(&parent_node, &node).is_err() {
            return Err(PostNodeError::InvalidTaxonomy);
        }
    }

    // Hard invariant 3: Global Sentinel ethical pre-filter must pass.
    if GlobalSentinel::evaluate(&*store, &state.root_axiom_id, &node).is_err() {
        return Err(PostNodeError::PolicyViolation);
    }

    // Weighted epistemic audit: graded merit scoring replaces binary jury consensus.
    let scorecard = state
        .jury
        .evaluate_node_weighted(&node)
        .await
        .map_err(|_| PostNodeError::Internal)?;

    if !scorecard.is_approved() {
        return Err(PostNodeError::JuryRejected(scorecard));
    }

    // Persist only once every gate has passed.
    if store.put_node(&node).is_err() {
        return Err(PostNodeError::Internal);
    }

    Ok(StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use eontree_core::NodeType;
    use tempfile::TempDir;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[5u8; 32])
    }

    fn signed_node(key: &SigningKey, parent_id: Option<NodeId>, title: &str) -> DecisionNode {
        signed_node_with_type(key, parent_id, NodeType::Innovation, title)
    }

    fn signed_node_with_type(
        key: &SigningKey,
        parent_id: Option<NodeId>,
        node_type: NodeType,
        title: &str,
    ) -> DecisionNode {
        let author_pubkey = hex::encode(key.verifying_key().to_bytes());
        let mut node = DecisionNode::new(
            parent_id,
            node_type,
            title.into(),
            "content".into(),
            vec![],
            author_pubkey,
        );
        node.sign(key);
        node
    }

    fn test_state() -> (Arc<AppState>, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = RedbStore::open(dir.path().join("test.redb")).expect("failed to open store");
        let state = Arc::new(AppState {
            store: Mutex::new(store),
            jury: LLMJudgeClient::new([
                "http://judge-alpha.invalid".into(),
                "http://judge-beta.invalid".into(),
                "http://judge-gamma.invalid".into(),
            ]),
            root_axiom_id: NodeId("root_axiom".into()),
        });
        (state, dir)
    }

    #[tokio::test]
    async fn get_node_handler_returns_404_when_missing() {
        let (state, _dir) = test_state();

        let result = get_node_handler(State(state), Path("missing".into())).await;

        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn post_then_get_node_round_trip() {
        let (state, _dir) = test_state();
        let node = signed_node(&test_key(), None, "Root Node");

        let post_result = post_node_handler(State(state.clone()), Json(node.clone())).await;
        assert!(matches!(post_result, Ok(StatusCode::CREATED)));

        let fetched = get_node_handler(State(state), Path(node.id.0.clone()))
            .await
            .unwrap();
        assert_eq!(fetched.0.id, node.id);
    }

    #[tokio::test]
    async fn post_node_handler_rejects_invalid_signature() {
        let (state, _dir) = test_state();
        let mut node = signed_node(&test_key(), None, "Root Node");
        node.content = "tampered".into();

        let result = post_node_handler(State(state), Json(node)).await;

        assert!(matches!(result, Err(PostNodeError::InvalidSignature)));
    }

    #[tokio::test]
    async fn get_children_handler_returns_children_of_parent() {
        let (state, _dir) = test_state();
        let key = test_key();

        // Create a Problem node as the parent (Root/Barrier require a stricter
        // 0.9 jury threshold that the mock judge scaffolding intentionally
        // doesn't clear, so Problem->Problem decomposition is used here instead).
        let parent = signed_node_with_type(&key, None, NodeType::Problem, "Parent");

        post_node_handler(State(state.clone()), Json(parent.clone()))
            .await
            .unwrap();

        // Create a Problem sub-problem under the parent
        let child =
            signed_node_with_type(&key, Some(parent.id.clone()), NodeType::Problem, "Child");

        post_node_handler(State(state.clone()), Json(child.clone()))
            .await
            .unwrap();

        let children = get_children_handler(State(state), Path(parent.id.0.clone()))
            .await
            .unwrap();

        assert_eq!(children.0.len(), 1);
        assert_eq!(children.0[0].id, child.id);
    }

    #[tokio::test]
    async fn get_children_handler_returns_empty_for_childless_node() {
        let (state, _dir) = test_state();
        let node = signed_node(&test_key(), None, "Lonely");
        post_node_handler(State(state.clone()), Json(node.clone()))
            .await
            .unwrap();

        let children = get_children_handler(State(state), Path(node.id.0.clone()))
            .await
            .unwrap();

        assert!(children.0.is_empty());
    }
}
