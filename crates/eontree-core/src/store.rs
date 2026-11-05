use crate::{DecisionNode, GraphStore, NodeId};
use redb::ReadableDatabase;
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
}
