use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use eontree_core::{DecisionNode, GraphStore, NodeId, NodeStatus, NodeType, store::RedbStore};
use eontree_jury::{GlobalSentinel, LLMJudgeClient, NodeScorecard};
use std::sync::Arc;
use tokio::sync::Mutex;

// Templates for the mandatory adversarial counter-nodes spawned alongside every
// Innovation/Problem proposal: an objection, a failure mode, and a competing
// alternative, each left as a PendingOpposition stub for the community to flesh
// out. These are deterministic, server-authored content, so they are written
// directly to the store rather than re-entering the public ingestion pipeline
// (no Sentinel/taxonomy re-check against the submitter's own node).
const ADVERSARIAL_TEMPLATES: [(&str, NodeType, &str); 3] = [
    (
        "Objections to: ",
        NodeType::Barrier,
        "Potential drawbacks, ethical concerns, or logical flaws.",
    ),
    (
        "Failure Modes of: ",
        NodeType::Barrier,
        "Conditions under which this approach breaks down catastrophically.",
    ),
    (
        "Competing Alternatives to: ",
        NodeType::Unknown,
        "Alternative methods achieving the same objective.",
    ),
];

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

struct AppState {
    store: Mutex<RedbStore>,
    jury: LLMJudgeClient,
    root_axiom_id: NodeId,
    // Signs automatically generated adversarial stub nodes. Every node must
    // carry a valid self-contained signature, so the system signs its own
    // stubs rather than bypassing verification for them.
    system_signing_key: ed25519_dalek::SigningKey,
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
        system_signing_key: generate_system_signing_key(),
    });

    // 2. Build Axum router
    let app = Router::new()
        .route("/nodes/todos", get(get_todos_handler))
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

fn generate_system_signing_key() -> ed25519_dalek::SigningKey {
    use ed25519_dalek::SigningKey;
    use getrandom::{SysRng, rand_core::UnwrapErr};

    let mut csprng = UnwrapErr(SysRng);
    SigningKey::generate(&mut csprng)
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

// Lists the community-opposition "todo queue": auto-generated stubs waiting
// for a contributor to flesh out and claim.
async fn get_todos_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<DecisionNode>>, StatusCode> {
    let store = state.store.lock().await;

    match store.list_by_status(NodeStatus::PendingOpposition) {
        Ok(nodes) => Ok(Json(nodes)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn spawn_mandatory_counter_nodes<S: GraphStore>(
    store: &mut S,
    parent_node: &DecisionNode,
    system_signing_key: &ed25519_dalek::SigningKey,
) -> Result<Vec<NodeId>, S::Error> {
    let system_pubkey = hex::encode(system_signing_key.verifying_key().to_bytes());

    let mut generated_ids = Vec::with_capacity(ADVERSARIAL_TEMPLATES.len());
    for (prefix, node_type, default_content) in ADVERSARIAL_TEMPLATES {
        let title = format!("{prefix}{}", parent_node.title);
        let mut counter_node = DecisionNode::new(
            Some(parent_node.id.clone()),
            node_type,
            title,
            default_content.to_string(),
            vec![],
            system_pubkey.clone(),
        );
        counter_node.status = NodeStatus::PendingOpposition;
        counter_node.sign(system_signing_key);

        store.put_node(&counter_node)?;
        generated_ids.push(counter_node.id);
    }

    Ok(generated_ids)
}

async fn post_node_handler(
    State(state): State<Arc<AppState>>,
    Json(mut node): Json<DecisionNode>,
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

    // Automatically spawn the mandatory adversarial counter-nodes (objections,
    // failure modes, competing alternatives) for primary proposals, and link
    // them back onto the parent record.
    if matches!(node.node_type, NodeType::Innovation | NodeType::Problem) {
        let counter_ids =
            spawn_mandatory_counter_nodes(&mut *store, &node, &state.system_signing_key)
                .map_err(|_| PostNodeError::Internal)?;
        node.counter_node_ids = counter_ids;
        store.put_node(&node).map_err(|_| PostNodeError::Internal)?;
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
            system_signing_key: SigningKey::from_bytes(&[6u8; 32]),
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

        // The parent also spawns 3 adversarial counter-nodes, but only the
        // explicitly posted child should show up here.
        assert!(children.0.iter().any(|c| c.id == child.id));
        assert_eq!(children.0.iter().filter(|c| c.id == child.id).count(), 1);
    }

    #[tokio::test]
    async fn get_children_handler_returns_empty_for_childless_node() {
        let (state, _dir) = test_state();
        // Unknown nodes don't trigger adversarial counter-node spawning and
        // clear the (lower) jury threshold for their type, so a freshly
        // posted one genuinely has no children.
        let node = signed_node_with_type(&test_key(), None, NodeType::Unknown, "Lonely");
        post_node_handler(State(state.clone()), Json(node.clone()))
            .await
            .unwrap();

        let children = get_children_handler(State(state), Path(node.id.0.clone()))
            .await
            .unwrap();

        assert!(children.0.is_empty());
    }

    #[tokio::test]
    async fn post_node_handler_spawns_counter_nodes_for_innovation() {
        let (state, _dir) = test_state();
        let node = signed_node(&test_key(), None, "New Approach");

        post_node_handler(State(state.clone()), Json(node.clone()))
            .await
            .unwrap();

        let fetched = get_node_handler(State(state.clone()), Path(node.id.0.clone()))
            .await
            .unwrap();
        assert_eq!(fetched.0.counter_node_ids.len(), 3);

        let children = get_children_handler(State(state), Path(node.id.0.clone()))
            .await
            .unwrap();
        assert_eq!(children.0.len(), 3);
        assert!(
            children
                .0
                .iter()
                .all(|c| c.status == NodeStatus::PendingOpposition)
        );
    }

    #[tokio::test]
    async fn get_todos_handler_lists_pending_opposition_stubs() {
        let (state, _dir) = test_state();
        let node = signed_node(&test_key(), None, "New Approach");

        post_node_handler(State(state.clone()), Json(node.clone()))
            .await
            .unwrap();

        let todos = get_todos_handler(State(state)).await.unwrap();
        assert_eq!(todos.0.len(), 3);
        assert!(
            todos
                .0
                .iter()
                .all(|t| t.status == NodeStatus::PendingOpposition)
        );
    }
}
