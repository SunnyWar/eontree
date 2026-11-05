use crate::{DecisionNode, GraphStore, NodeId, NodeStatus};
use redb::ReadableDatabase;
use redb::ReadableTable;
use redb::{Database, TableDefinition};
use std::path::Path;

const NODES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("eontree_nodes");
// Composite key: (parent_id, child_id) -> ()
const CHILDREN_INDEX: TableDefinition<(&str, &str), ()> =
    TableDefinition::new("eontree_children_index");

pub struct RedbStore {
    db: Database,
}

impl RedbStore {
    /// Opens or creates a redb database at the specified file path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, redb::Error> {
        let db = Database::create(path)?;

        // Initialize tables during startup
        let write_txn = db.begin_write()?;
        write_txn.open_table(NODES_TABLE)?;
        write_txn.open_table(CHILDREN_INDEX)?;
        write_txn.commit()?;

        Ok(Self { db })
    }
}

impl GraphStore for RedbStore {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn get_node(&self, id: &NodeId) -> Result<Option<DecisionNode>, Self::Error> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NODES_TABLE)?;

        if let Some(bytes) = table.get(id.0.as_str())? {
            let node: DecisionNode = serde_json::from_slice(bytes.value())?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    fn put_node(&mut self, node: &DecisionNode) -> Result<(), Self::Error> {
        // 1. Verify cryptographic integrity before writing
        node.verify()?;

        let serialized = serde_json::to_vec(node)?;
        let write_txn = self.db.begin_write()?;

        {
            let mut nodes_table = write_txn.open_table(NODES_TABLE)?;
            nodes_table.insert(node.id.0.as_str(), serialized.as_slice())?;

            // If there's a parent, update the children adjacency index
            if let Some(ref parent_id) = node.parent_id {
                let mut children_table = write_txn.open_table(CHILDREN_INDEX)?;
                children_table.insert((parent_id.0.as_str(), node.id.0.as_str()), ())?;
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    fn get_children(&self, id: &NodeId) -> Result<Vec<DecisionNode>, Self::Error> {
        let read_txn = self.db.begin_read()?;
        let children_table = read_txn.open_table(CHILDREN_INDEX)?;
        let nodes_table = read_txn.open_table(NODES_TABLE)?;

        let mut child_nodes = Vec::new();

        // Range query over composite keys starting with the parent_id
        let lower_bound = (id.0.as_str(), "");
        let upper_bound = (id.0.as_str(), "\u{10ffff}"); // max unicode char

        for entry in children_table.range(lower_bound..=upper_bound)? {
            let entry = entry?;
            let key_guard = entry.0;
            let (_, child_id_str) = key_guard.value();

            if let Some(bytes) = nodes_table.get(child_id_str)? {
                let node: DecisionNode = serde_json::from_slice(bytes.value())?;
                child_nodes.push(node);
            }
        }

        Ok(child_nodes)
    }

    fn list_by_status(&self, status: NodeStatus) -> Result<Vec<DecisionNode>, Self::Error> {
        let read_txn = self.db.begin_read()?;
        let nodes_table = read_txn.open_table(NODES_TABLE)?;

        let mut matches = Vec::new();
        for entry in nodes_table.iter()? {
            let (_, value_guard) = entry?;
            let node: DecisionNode = serde_json::from_slice(value_guard.value())?;
            if node.status == status {
                matches.push(node);
            }
        }

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeType;
    use ed25519_dalek::SigningKey;
    use tempfile::TempDir;

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[3u8; 32])
    }

    fn signed_node(key: &SigningKey, parent_id: Option<NodeId>, title: &str) -> DecisionNode {
        let author_pubkey = hex::encode(key.verifying_key().to_bytes());
        let mut node = DecisionNode::new(
            parent_id,
            NodeType::Problem,
            title.into(),
            "content".into(),
            vec![],
            author_pubkey,
        );
        node.sign(key);
        node
    }

    fn open_temp_store() -> (RedbStore, TempDir) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let store = RedbStore::open(dir.path().join("test.redb")).expect("failed to open store");
        (store, dir)
    }

    #[test]
    fn put_and_get_node_round_trip() {
        let (mut store, _dir) = open_temp_store();
        let node = signed_node(&test_key(), None, "Root");

        store.put_node(&node).expect("put_node should succeed");

        let fetched = store
            .get_node(&node.id)
            .expect("get_node should succeed")
            .expect("node should exist");
        assert_eq!(fetched.id, node.id);
        assert_eq!(fetched.title, "Root");
    }

    #[test]
    fn get_node_missing_returns_none() {
        let (store, _dir) = open_temp_store();
        let missing = NodeId("does-not-exist".into());
        assert!(store.get_node(&missing).unwrap().is_none());
    }

    #[test]
    fn put_node_rejects_invalid_signature() {
        let (mut store, _dir) = open_temp_store();
        let mut node = signed_node(&test_key(), None, "Root");
        node.content = "tampered".into();

        let err = store.put_node(&node).unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn get_children_returns_only_direct_children() {
        let (mut store, _dir) = open_temp_store();
        let key = test_key();

        let parent = signed_node(&key, None, "Parent");
        store.put_node(&parent).unwrap();

        let child = signed_node(&key, Some(parent.id.clone()), "Child");
        store.put_node(&child).unwrap();

        let unrelated = signed_node(&key, None, "Unrelated");
        store.put_node(&unrelated).unwrap();

        let children = store.get_children(&parent.id).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);
    }

    #[test]
    fn get_children_returns_empty_vec_for_childless_node() {
        let (mut store, _dir) = open_temp_store();
        let node = signed_node(&test_key(), None, "Lonely");
        store.put_node(&node).unwrap();

        assert!(store.get_children(&node.id).unwrap().is_empty());
    }

    #[test]
    fn list_by_status_returns_only_matching_nodes() {
        let (mut store, _dir) = open_temp_store();
        let key = test_key();

        let active = signed_node(&key, None, "Active Node");
        store.put_node(&active).unwrap();

        let mut stub = signed_node(&key, None, "Stub Node");
        stub.status = NodeStatus::PendingOpposition;
        store.put_node(&stub).unwrap();

        let pending = store.list_by_status(NodeStatus::PendingOpposition).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, stub.id);

        let active_nodes = store.list_by_status(NodeStatus::Active).unwrap();
        assert_eq!(active_nodes.len(), 1);
        assert_eq!(active_nodes[0].id, active.id);
    }
}
