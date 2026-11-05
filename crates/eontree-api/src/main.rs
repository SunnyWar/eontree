use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use eontree_core::{DecisionNode, GraphStore, NodeId, store::RedbStore};
use eontree_jury::{GlobalSentinel, LLMJudgeClient};
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

async fn post_node_handler(
    State(state): State<Arc<AppState>>,
    Json(node): Json<DecisionNode>,
) -> Result<StatusCode, StatusCode> {
    if node.verify().is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut store = state.store.lock().await;

    // 1. Run Global Sentinel ethical pre-filter
    if GlobalSentinel::evaluate(&*store, &state.root_axiom_id, &node).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // 2. Execute Triple-LLM jury consensus audit
    if state.jury.audit_node(&node).await.is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // 3. Commit to persistent store via trait method
    if store.put_node(&node).is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use tempfile::TempDir;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[5u8; 32])
    }

    fn signed_node(key: &SigningKey, parent_id: Option<NodeId>, title: &str) -> DecisionNode {
        let author_pubkey = hex::encode(key.verifying_key().to_bytes());
        let mut node = DecisionNode::new(
            parent_id,
            eontree_core::NodeType::Innovation,
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
        assert_eq!(post_result.unwrap(), StatusCode::CREATED);

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

        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_children_handler_returns_children_of_parent() {
        let (state, _dir) = test_state();
        let key = test_key();
        let parent = signed_node(&key, None, "Parent");
        post_node_handler(State(state.clone()), Json(parent.clone()))
            .await
            .unwrap();

        let child = signed_node(&key, Some(parent.id.clone()), "Child");
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
