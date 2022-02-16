use crate::utxo::rpc_clients::ElectrumBlockHeader;
use crate::utxo::utxo_block_header_storage::{BlockHeaderStorage, BlockHeaderStorageError};
use async_trait::async_trait;
use chain::BlockHeader;
use common::async_blocking;
use common::mm_error::MmError;
use db_common::sqlite::rusqlite::Error as SqlError;
use db_common::sqlite::rusqlite::{Connection, Row, ToSql, NO_PARAMS};
use db_common::sqlite::validate_table_name;
use std::sync::{Arc, Mutex};

const CHECK_TABLE_EXISTS_SQL: &str = "SELECT name FROM sqlite_master WHERE type='table' AND name=?1;";

fn block_headers_cache_table(ticker: &str) -> String { ticker.to_owned() + "_block_headers_cache" }
fn create_block_header_cache_table_sql(for_coin: &str) -> Result<String, MmError<SqlError>> {
    let table_name = block_headers_cache_table(for_coin);
    validate_table_name(&table_name)?;

    let sql = "CREATE TABLE IF NOT EXISTS ".to_owned()
        + &table_name
        + " (
        block_height INTEGER NOT NULL UNIQUE,
        hex TEXT NOT NULL
    );";

    Ok(sql)
}

fn insert_block_header_in_cache_sql(for_coin: &str) -> Result<String, MmError<SqlError>> {
    let table_name = block_headers_cache_table(for_coin);
    validate_table_name(&table_name)?;

    // We can simply ignore the repetitive attempt to insert the same block_height
    let sql = "INSERT OR IGNORE INTO ".to_owned() + &table_name + " (block_height, hex) VALUES (?1, ?2);";

    Ok(sql)
}

#[derive(Clone)]
pub struct SqliteBlockHeadersStorage(pub Arc<Mutex<Connection>>);

fn query_single_row<T, P, F>(
    conn: &Connection,
    query: &str,
    params: P,
    map_fn: F,
) -> Result<Option<T>, MmError<SqlError>>
where
    P: IntoIterator,
    P::Item: ToSql,
    F: FnOnce(&Row<'_>) -> Result<T, SqlError>,
{
    let maybe_result = conn.query_row(query, params, map_fn);
    if let Err(SqlError::QueryReturnedNoRows) = maybe_result {
        return Ok(None);
    }

    let result = maybe_result?;
    Ok(Some(result))
}

fn string_from_row(row: &Row<'_>) -> Result<String, SqlError> { row.get(0) }

impl BlockHeaderStorageError for SqlError {}

#[async_trait]
impl BlockHeaderStorage for SqliteBlockHeadersStorage {
    type Error = SqlError;

    async fn init(&self, for_coin: &str) -> Result<(), MmError<Self::Error>> {
        let selfi = self.clone();
        let sql_cache = create_block_header_cache_table_sql(for_coin)?;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            conn.execute(&sql_cache, NO_PARAMS).map(|_| ())?;
            Ok(())
        })
        .await
    }

    async fn is_initialized_for(&self, for_coin: &str) -> Result<bool, MmError<Self::Error>> {
        let block_headers_cache_table = block_headers_cache_table(for_coin);
        validate_table_name(&block_headers_cache_table)?;

        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let cache_initialized = query_single_row(
                &conn,
                CHECK_TABLE_EXISTS_SQL,
                [block_headers_cache_table],
                string_from_row,
            )?;
            Ok(cache_initialized.is_some())
        })
        .await
    }

    async fn add_block_headers_to_storage(
        &self,
        for_coin: &str,
        headers: Vec<ElectrumBlockHeader>,
    ) -> Result<(), MmError<Self::Error>> {
        todo!()
    }

    async fn get_block_header(&self, for_coin: &str, height: u64) -> Result<Option<BlockHeader>, MmError<Self::Error>> {
        todo!()
    }

    async fn get_block_header_raw(&self, for_coin: &str, height: u64) -> Result<Option<String>, MmError<Self::Error>> {
        todo!()
    }
}

#[cfg(test)]
impl SqliteBlockHeadersStorage {
    pub fn in_memory() -> Self {
        SqliteBlockHeadersStorage(Arc::new(Mutex::new(Connection::open_in_memory().unwrap())))
    }
}

#[cfg(test)]
mod sql_block_headers_storage_tests {
    use super::*;
    use common::block_on;
    use std::num::NonZeroUsize;

    #[test]
    fn test_init_collection() {
        let for_coin = "init_collection";
        let storage = SqliteBlockHeadersStorage::in_memory();
        let initialized = block_on(storage.is_initialized_for(for_coin)).unwrap();
        assert!(!initialized);

        block_on(storage.init(for_coin)).unwrap();
        // repetitive init must not fail
        block_on(storage.init(for_coin)).unwrap();

        let initialized = block_on(storage.is_initialized_for(for_coin)).unwrap();
        assert!(initialized);
    }
}
