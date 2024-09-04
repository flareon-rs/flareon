use flareon::db::DbField;
use sea_query::Value;

use crate::db::{ColumnType, FromDbValue, Result, SqliteValueRef, SqlxValueRef, ToDbValue};

macro_rules! impl_db_field {
    ($ty:ty, $column_type:ident) => {
        impl DbField for $ty {
            const TYPE: ColumnType = ColumnType::$column_type;
        }

        impl FromDbValue for $ty {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<$ty>()
            }
        }

        impl ToDbValue for $ty {
            fn as_sea_query_value(&self) -> Value {
                self.clone().into()
            }
        }
    };
}

impl_db_field!(bool, Boolean);
impl_db_field!(i8, TinyInteger);
impl_db_field!(i16, SmallInteger);
impl_db_field!(i32, Integer);
impl_db_field!(i64, BigInteger);
impl_db_field!(u8, TinyUnsignedInteger);
impl_db_field!(u16, SmallUnsignedInteger);
impl_db_field!(u32, UnsignedInteger);
impl_db_field!(u64, BigUnsignedInteger);
impl_db_field!(f32, Float);
impl_db_field!(f64, Double);
impl_db_field!(chrono::NaiveDate, Date);
impl_db_field!(chrono::NaiveTime, Time);
impl_db_field!(chrono::NaiveDateTime, DateTime);
impl_db_field!(chrono::DateTime<chrono::FixedOffset>, TimestampWithTimeZone);
impl_db_field!(String, Text);

impl ToDbValue for &str {
    fn as_sea_query_value(&self) -> Value {
        (*self).to_string().into()
    }
}
