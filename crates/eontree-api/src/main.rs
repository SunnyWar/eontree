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
