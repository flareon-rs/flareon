use flareon::db::DatabaseField;
use sea_query::Value;

#[cfg(feature = "mysql")]
use crate::db::impl_mysql::MySqlValueRef;
#[cfg(feature = "postgres")]
use crate::db::impl_postgres::PostgresValueRef;
#[cfg(feature = "sqlite")]
use crate::db::impl_sqlite::SqliteValueRef;
use crate::db::{
    ColumnType, DatabaseError, FromDbValue, LimitedString, Result, SqlxValueRef, ToDbValue,
};

macro_rules! impl_from_sqlite_default {
    () => {
        #[cfg(feature = "sqlite")]
        fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_from_postgres_default {
    () => {
        #[cfg(feature = "postgres")]
        fn from_postgres(value: PostgresValueRef) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_from_mysql_default {
    () => {
        #[cfg(feature = "mysql")]
        fn from_mysql(value: MySqlValueRef) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_to_db_value_default {
    ($ty:ty) => {
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

macro_rules! impl_db_field {
    ($ty:ty, $column_type:ident) => {
        impl DatabaseField for $ty {
            const TYPE: ColumnType = ColumnType::$column_type;
        }

        impl FromDbValue for $ty {
            impl_from_sqlite_default!();

            impl_from_postgres_default!();

            impl_from_mysql_default!();
        }

        impl FromDbValue for Option<$ty> {
            impl_from_sqlite_default!();

            impl_from_postgres_default!();

            impl_from_mysql_default!();
        }

        impl_to_db_value_default!($ty);
    };
}

macro_rules! impl_db_field_with_postgres_int_cast {
    ($dest_ty:ty, $src_ty:ty, $column_type:ident) => {
        impl DatabaseField for $dest_ty {
            const TYPE: ColumnType = ColumnType::$column_type;
        }

        impl FromDbValue for $dest_ty {
            impl_from_sqlite_default!();

            impl_from_mysql_default!();

            #[cfg(feature = "postgres")]
            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
                #[allow(clippy::cast_possible_truncation)]
                #[allow(clippy::cast_sign_loss)]
                value.get::<$src_ty>().map(|v| v as $dest_ty)
            }
        }

        impl FromDbValue for Option<$dest_ty> {
            impl_from_sqlite_default!();

            impl_from_mysql_default!();

            #[cfg(feature = "postgres")]
            fn from_postgres(value: PostgresValueRef) -> Result<Self> {
                #[allow(clippy::cast_possible_truncation)]
                #[allow(clippy::cast_sign_loss)]
                value
                    .get::<Option<$src_ty>>()
                    .map(|v| v.map(|v| v as $dest_ty))
            }
        }

        impl_to_db_value_default!($dest_ty);
    };
}

impl_db_field!(bool, Boolean);
impl_db_field!(i16, SmallInteger);
impl_db_field!(i32, Integer);
impl_db_field!(i64, BigInteger);
impl_db_field_with_postgres_int_cast!(i8, i16, TinyInteger);
impl_db_field_with_postgres_int_cast!(u8, i16, TinyUnsignedInteger);
impl_db_field_with_postgres_int_cast!(u16, i16, SmallUnsignedInteger);
impl_db_field_with_postgres_int_cast!(u32, i32, UnsignedInteger);
impl_db_field_with_postgres_int_cast!(u64, i64, BigUnsignedInteger);
impl_db_field!(f32, Float);
impl_db_field!(f64, Double);
impl_db_field!(chrono::NaiveDate, Date);
impl_db_field!(chrono::NaiveTime, Time);
impl_db_field!(chrono::NaiveDateTime, DateTime);
impl_db_field!(String, Text);
impl_db_field!(Vec<u8>, Blob);

impl ToDbValue for &str {
    fn to_sea_query_value(&self) -> Value {
        (*self).to_string().into()
    }
}

impl DatabaseField for chrono::DateTime<chrono::FixedOffset> {
    const TYPE: ColumnType = ColumnType::DateTimeWithTimeZone;
}

impl FromDbValue for chrono::DateTime<chrono::FixedOffset> {
    impl_from_sqlite_default!();

    impl_from_postgres_default!();

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef) -> Result<Self> {
        Ok(value.get::<chrono::DateTime<chrono::Utc>>()?.fixed_offset())
    }
}
impl FromDbValue for Option<chrono::DateTime<chrono::FixedOffset>> {
    impl_from_sqlite_default!();

    impl_from_postgres_default!();

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef) -> Result<Self> {
        Ok(value
            .get::<Option<chrono::DateTime<chrono::Utc>>>()?
            .map(|dt| dt.fixed_offset()))
    }
}

impl_to_db_value_default!(chrono::DateTime<chrono::FixedOffset>);

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
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }
}

impl<const LIMIT: u32> ToDbValue for LimitedString<LIMIT> {
    fn to_sea_query_value(&self) -> Value {
        self.0.clone().into()
    }
}

impl<const LIMIT: u32> ToDbValue for Option<LimitedString<LIMIT>> {
    fn to_sea_query_value(&self) -> Value {
        self.clone().map(|s| s.0).into()
    }
}
