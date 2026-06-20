//! Tabular Cypher results for `esgraphd query`.

use lbug::Connection;

use crate::cypher::value_to_string;
use crate::error::StoreError;
use crate::store::GraphStore;

/// Column names and stringified row values from a RETURN query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    /// Result column names.
    pub columns: Vec<String>,
    /// Rows as string cells (`NULL` → empty string).
    pub rows: Vec<Vec<String>>,
}

impl GraphStore {
    /// Execute a read-only Cypher query and return all rows as strings for CLI display.
    pub fn query_tabular(&self, cypher: &str) -> Result<QueryResult, StoreError> {
        let conn = Connection::new(self.db())?;
        let result = conn.query(cypher)?;

        let columns = result.get_column_names();
        let rows = result
            .map(|row| row.iter().map(value_to_string).collect())
            .collect();

        Ok(QueryResult { columns, rows })
    }
}
