use crate::db::sea_query_db::impl_sea_query_db_backend;

impl_sea_query_db_backend!(DatabasePostgres: sqlx::postgres::Postgres, sqlx::postgres::PgPool, PostgresRow, PostgresValueRef, sea_query::PostgresQueryBuilder);

impl DatabasePostgres {
    fn prepare_values(values: &mut sea_query_binder::SqlxValues) {
        for value in &mut values.0 .0 {
            Self::tinyint_to_smallint(value);
            Self::unsigned_to_signed(value);
        }
    }

    /// PostgreSQL does only support 2+ bytes integers, so we need to convert
    /// i8/u8 to i16/u16. Otherwise, sqlx will convert them internally to `char`
    /// and we'll get an error.
    fn tinyint_to_smallint(value: &mut sea_query::Value) {
        if let sea_query::Value::TinyInt(num) = value {
            *value = sea_query::Value::SmallInt(num.map(i16::from));
        } else if let sea_query::Value::TinyUnsigned(num) = value {
            *value = sea_query::Value::SmallInt(num.map(i16::from));
        }
    }

    /// PostgreSQL doesn't support unsigned integers, so we need to convert
    /// them to signed integers.
    fn unsigned_to_signed(value: &mut sea_query::Value) {
        #[allow(clippy::cast_possible_wrap)]
        if let sea_query::Value::SmallUnsigned(num) = value {
            *value = sea_query::Value::SmallInt(num.map(|v| v as i16));
        } else if let sea_query::Value::Unsigned(num) = value {
            *value = sea_query::Value::Int(num.map(|v| v as i32));
        } else if let sea_query::Value::BigUnsigned(num) = value {
            *value = sea_query::Value::BigInt(num.map(|v| v as i64));
        }
    }

    pub(super) fn sea_query_column_type_for(
        &self,
        column_type: crate::db::ColumnType,
    ) -> sea_query::ColumnType {
        sea_query::ColumnType::from(column_type)
    }
}
