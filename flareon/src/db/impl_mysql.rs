use crate::db::sea_query_db::impl_sea_query_db_backend;
use crate::db::ColumnType;

impl_sea_query_db_backend!(DatabaseMySql: sqlx::mysql::MySql, sqlx::mysql::MySqlPool, MySqlRow, MySqlValueRef, sea_query::MysqlQueryBuilder);

impl DatabaseMySql {
    #[allow(clippy::unused_async)]
    async fn init(&self) -> crate::db::Result<()> {
        Ok(())
    }

    fn prepare_values(_values: &mut sea_query_binder::SqlxValues) {
        // No changes are needed for MySQL
    }

    fn last_inserted_row_id_for(result: &sqlx::mysql::MySqlQueryResult) -> Option<u64> {
        Some(result.last_insert_id())
    }

    pub(super) fn sea_query_column_type_for(
        &self,
        column_type: ColumnType,
    ) -> sea_query::ColumnType {
        match column_type {
            ColumnType::DateTime | ColumnType::DateTimeWithTimeZone => {
                return sea_query::ColumnType::custom("DATETIME(6)");
            }
            _ => {}
        }

        sea_query::ColumnType::from(column_type)
    }
}
