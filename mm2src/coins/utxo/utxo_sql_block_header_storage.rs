use crate::utxo::rpc_clients::ElectrumBlockHeader;
use crate::utxo::utxo_block_header_storage::{BlockHeaderStorage, BlockHeaderStorageError};
use async_trait::async_trait;
use chain::BlockHeader;
use common::async_blocking;
use common::mm_error::MmError;
use db_common::sqlite::rusqlite::types::Type;
use db_common::sqlite::rusqlite::Error as SqlError;
use db_common::sqlite::rusqlite::{Connection, Row, ToSql, NO_PARAMS};
use db_common::sqlite::validate_table_name;
use serialization::deserialize;
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

fn get_block_header_by_height(for_coin: &str) -> Result<String, MmError<SqlError>> {
    let table_name = block_headers_cache_table(for_coin);
    validate_table_name(&table_name)?;

    let sql = "SELECT hex FROM ".to_owned() + &table_name + " WHERE block_height=?1;";

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
        let for_coin = for_coin.to_owned();
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            for header in headers {
                match header {
                    ElectrumBlockHeader::V12(_) => {},
                    ElectrumBlockHeader::V14(h) => {
                        let block_hex = format!("{:02x}", h.hex);
                        let block_cache_params = [&h.height.to_string(), &block_hex];
                        sql_transaction.execute(&insert_block_header_in_cache_sql(&for_coin)?, block_cache_params)?;
                    },
                }
            }
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_block_header(&self, for_coin: &str, height: u64) -> Result<Option<BlockHeader>, MmError<Self::Error>> {
        if let Some(header_raw) = self.get_block_header_raw(for_coin, height).await? {
            let header_bytes =
                hex::decode(header_raw).map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))?;
            let header: BlockHeader = deserialize(header_bytes.as_slice())
                .map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))?;
            return Ok(Some(header));
        }
        Ok(None)
    }

    async fn get_block_header_raw(&self, for_coin: &str, height: u64) -> Result<Option<String>, MmError<Self::Error>> {
        let params = [height.to_string()];
        let sql = get_block_header_by_height(for_coin)?;
        let selfi = self.clone();

        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, params, string_from_row)
        })
        .await
    }
}

#[cfg(test)]
impl SqliteBlockHeadersStorage {
    pub fn in_memory() -> Self {
        SqliteBlockHeadersStorage(Arc::new(Mutex::new(Connection::open_in_memory().unwrap())))
    }

    fn is_table_empty(&self, table_name: &str) -> bool {
        validate_table_name(table_name).unwrap();
        let sql = "SELECT COUNT(block_height) FROM ".to_owned() + table_name + ";";
        let conn = self.0.lock().unwrap();
        let rows_count: u32 = conn.query_row(&sql, NO_PARAMS, |row| row.get(0)).unwrap();
        rows_count == 0
    }
}

#[cfg(test)]
mod sql_block_headers_storage_tests {
    use super::*;
    use crate::utxo::rpc_clients::ElectrumBlockHeaderV14;
    use common::block_on;
    use hex::FromHex;
    use primitives::hash::H256;

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

    #[test]
    fn test_add_block_headers() {
        let for_coin = "insert";
        let storage = SqliteBlockHeadersStorage::in_memory();
        let table = block_headers_cache_table(for_coin);
        block_on(storage.init(for_coin)).unwrap();

        let initialized = block_on(storage.is_initialized_for(for_coin)).unwrap();
        assert!(initialized);

        let block_header = ElectrumBlockHeaderV14 {
            height: 520481,
            hex: "0000002076d41d3e4b0bfd4c0d3b30aa69fdff3ed35d85829efd04000000000000000000b386498b583390959d9bac72346986e3015e83ac0b54bc7747a11a494ac35c94bb3ce65a53fb45177f7e311c".into(),
        }.into();
        let headers = vec![ElectrumBlockHeader::V14(block_header)];
        block_on(storage.add_block_headers_to_storage(for_coin, headers)).unwrap();
        assert!(!storage.is_table_empty(&table));
    }

    #[test]
    fn test_get_block_header() {
        let for_coin = "get";
        let storage = SqliteBlockHeadersStorage::in_memory();
        let table = block_headers_cache_table(for_coin);
        block_on(storage.init(for_coin)).unwrap();

        let initialized = block_on(storage.is_initialized_for(for_coin)).unwrap();
        assert!(initialized);

        let block_header = ElectrumBlockHeaderV14 {
            height: 520481,
            hex: "0000002076d41d3e4b0bfd4c0d3b30aa69fdff3ed35d85829efd04000000000000000000b386498b583390959d9bac72346986e3015e83ac0b54bc7747a11a494ac35c94bb3ce65a53fb45177f7e311c".into(),
        }.into();
        let headers = vec![ElectrumBlockHeader::V14(block_header)];
        block_on(storage.add_block_headers_to_storage(for_coin, headers)).unwrap();
        assert!(!storage.is_table_empty(&table));

        let hex = block_on(storage.get_block_header_raw(for_coin, 520481))
            .unwrap()
            .unwrap();
        assert_eq!(hex, "0000002076d41d3e4b0bfd4c0d3b30aa69fdff3ed35d85829efd04000000000000000000b386498b583390959d9bac72346986e3015e83ac0b54bc7747a11a494ac35c94bb3ce65a53fb45177f7e311c".to_string());

        let block_header = block_on(storage.get_block_header(for_coin, 520481)).unwrap().unwrap();
        assert_eq!(
            block_header.hash(),
            H256::from_reversed_str("0000000000000000002e31d0714a5ab23100945ff87ba2d856cd566a3c9344ec")
        )
    }
}
