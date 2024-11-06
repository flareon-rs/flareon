use flareon::db::DatabaseField;
use sea_query::Value;

use crate::db::{
    ColumnType, DatabaseError, FromDbValue, LimitedString, Result, SqliteValueRef, SqlxValueRef,
    ToDbValue,
};

macro_rules! impl_db_field {
    ($ty:ty, $column_type:ident) => {
        impl DatabaseField for $ty {
            const TYPE: ColumnType = ColumnType::$column_type;
        }

        impl FromDbValue for $ty {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<$ty>()
            }
        }

        impl FromDbValue for Option<$ty> {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<Option<$ty>>()
            }
        }

        impl ToDbValue for $ty {
            fn to_sea_query_value(&self) -> Value {
                self.clone().into()
            }
        }

        impl ToDbValue for Option<$ty> {
            fn to_sea_query_value(&self) -> Value {
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
impl_db_field!(Vec<u8>, Blob);

impl ToDbValue for &str {
    fn to_sea_query_value(&self) -> Value {
        (*self).to_string().into()
    }
}

impl ToDbValue for Option<&str> {
    fn to_sea_query_value(&self) -> Value {
        self.map(ToString::to_string).into()
    }
}

impl<T: DatabaseField> DatabaseField for Option<T>
where
    Option<T>: ToDbValue + FromDbValue,
{
    const NULLABLE: bool = true;
    const TYPE: ColumnType = T::TYPE;
}

impl<const LIMIT: u32> DatabaseField for LimitedString<LIMIT> {
    const TYPE: ColumnType = ColumnType::String(LIMIT);
}

impl<const LIMIT: u32> FromDbValue for LimitedString<LIMIT> {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }
}

impl<const LIMIT: u32> ToDbValue for LimitedString<LIMIT> {
    fn to_sea_query_value(&self) -> Value {
        self.0.clone().into()
    }
}
