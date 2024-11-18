use flareon::db::DatabaseField;
use sea_query::Value;

use crate::db::{
    ColumnType, DatabaseError, FromDbValue, LimitedString, PostgresValueRef, Result,
    SqliteValueRef, SqlxValueRef, ToDbValue,
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

            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
                value.get::<$ty>()
            }
        }

        impl FromDbValue for Option<$ty> {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<Option<$ty>>()
            }

            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
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

macro_rules! impl_db_field_unsigned {
    ($ty:ty, $signed_ty:ty, $column_type:ident) => {
        impl DatabaseField for $ty {
            const TYPE: ColumnType = ColumnType::$column_type;
        }

        impl FromDbValue for $ty {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<$ty>()
            }

            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
                value.get::<$signed_ty>().map(|v| v as $ty)
            }
        }

        impl FromDbValue for Option<$ty> {
            fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
                value.get::<Option<$ty>>()
            }

            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
                value
                    .get::<Option<$signed_ty>>()
                    .map(|v| v.map(|v| v as $ty))
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

impl DatabaseField for i8 {
    const TYPE: ColumnType = ColumnType::TinyInteger;
}

impl FromDbValue for i8 {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        value.get::<i8>()
    }

    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        value.get::<i16>().map(|v| v as i8)
    }
}

impl FromDbValue for Option<i8> {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        value.get::<Option<i8>>()
    }

    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        value.get::<Option<i16>>().map(|v| v.map(|v| v as i8))
    }
}

impl ToDbValue for i8 {
    fn to_sea_query_value(&self) -> Value {
        (*self).into()
    }
}

impl ToDbValue for Option<i8> {
    fn to_sea_query_value(&self) -> Value {
        (*self).into()
    }
}

impl DatabaseField for u8 {
    const TYPE: ColumnType = ColumnType::TinyUnsignedInteger;
}

impl FromDbValue for u8 {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        value.get::<u8>()
    }

    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        value.get::<i16>().map(|v| v as u8)
    }
}

impl FromDbValue for Option<u8> {
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        value.get::<Option<u8>>()
    }

    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        value.get::<Option<i16>>().map(|v| v.map(|v| v as u8))
    }
}

impl ToDbValue for u8 {
    fn to_sea_query_value(&self) -> Value {
        (*self).into()
    }
}

impl ToDbValue for Option<u8> {
    fn to_sea_query_value(&self) -> Value {
        (*self).into()
    }
}

impl_db_field!(bool, Boolean);
impl_db_field!(i16, SmallInteger);
impl_db_field!(i32, Integer);
impl_db_field!(i64, BigInteger);
impl_db_field_unsigned!(u16, i16, SmallUnsignedInteger);
impl_db_field_unsigned!(u32, i32, UnsignedInteger);
impl_db_field_unsigned!(u64, i64, BigUnsignedInteger);
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

    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }
}

impl<const LIMIT: u32> ToDbValue for LimitedString<LIMIT> {
    fn to_sea_query_value(&self) -> Value {
        self.0.clone().into()
    }
}
