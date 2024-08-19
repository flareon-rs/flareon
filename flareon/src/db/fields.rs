use flareon::db::DbField;
use sea_query::Value;

use crate::db::{ColumnType, FromDbValue, ValueRef};

impl DbField for i32 {
    const TYPE: ColumnType = ColumnType::Integer;
}

impl FromDbValue<'_> for i32 {
    type SqlxType = i32;

    fn from_sqlx(value: Self::SqlxType) -> Self {
        value
    }
}

impl ValueRef for i32 {
    fn as_sea_query_value(&self) -> Value {
        (*self).into()
    }
}

impl DbField for String {
    const TYPE: ColumnType = ColumnType::Text;
}

impl FromDbValue<'_> for String {
    type SqlxType = String;

    fn from_sqlx(value: Self::SqlxType) -> Self {
        value
    }
}

impl ValueRef for String {
    fn as_sea_query_value(&self) -> Value {
        self.into()
    }
}
