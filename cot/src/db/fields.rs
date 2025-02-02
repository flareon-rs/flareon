//! `DatabaseField` implementations for common types.

#[cfg(feature = "mysql")]
use crate::db::impl_mysql::MySqlValueRef;
#[cfg(feature = "postgres")]
use crate::db::impl_postgres::PostgresValueRef;
#[cfg(feature = "sqlite")]
use crate::db::impl_sqlite::SqliteValueRef;
use crate::db::{
    Auto, ColumnType, DatabaseError, DatabaseField, DbFieldValue, DbValue, ForeignKey, FromDbValue,
    LimitedString, Model, PrimaryKey, Result, SqlxValueRef, ToDbFieldValue, ToDbValue,
};

macro_rules! impl_from_sqlite_default {
    () => {
        #[cfg(feature = "sqlite")]
        fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_from_postgres_default {
    () => {
        #[cfg(feature = "postgres")]
        fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_from_mysql_default {
    () => {
        #[cfg(feature = "mysql")]
        fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
            value.get::<Self>()
        }
    };
}

macro_rules! impl_to_db_value_default {
    ($ty:ty) => {
        impl ToDbValue for $ty {
            fn to_db_value(&self) -> DbValue {
                self.clone().into()
            }
        }

        impl ToDbValue for Option<$ty> {
            fn to_db_value(&self) -> DbValue {
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
            fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
                #[allow(clippy::cast_possible_truncation)]
                #[allow(clippy::cast_sign_loss)]
                value.get::<$src_ty>().map(|v| v as $dest_ty)
            }
        }

        impl FromDbValue for Option<$dest_ty> {
            impl_from_sqlite_default!();

            impl_from_mysql_default!();

            #[cfg(feature = "postgres")]
            fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
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
    fn to_db_value(&self) -> DbValue {
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
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
        Ok(value.get::<chrono::DateTime<chrono::Utc>>()?.fixed_offset())
    }
}
impl FromDbValue for Option<chrono::DateTime<chrono::FixedOffset>> {
    impl_from_sqlite_default!();

    impl_from_postgres_default!();

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
        Ok(value
            .get::<Option<chrono::DateTime<chrono::Utc>>>()?
            .map(|dt| dt.fixed_offset()))
    }
}

impl_to_db_value_default!(chrono::DateTime<chrono::FixedOffset>);

impl ToDbValue for Option<&str> {
    fn to_db_value(&self) -> DbValue {
        self.map(ToString::to_string).into()
    }
}

impl<T: DatabaseField> DatabaseField for Option<T>
where
    Option<T>: ToDbFieldValue + FromDbValue,
{
    const NULLABLE: bool = true;
    const TYPE: ColumnType = T::TYPE;
}

impl<const LIMIT: u32> DatabaseField for LimitedString<LIMIT> {
    const TYPE: ColumnType = ColumnType::String(LIMIT);
}

impl<const LIMIT: u32> FromDbValue for LimitedString<LIMIT> {
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
        let str = value.get::<String>()?;
        Self::new(str).map_err(DatabaseError::value_decode)
    }
}

impl<const LIMIT: u32> ToDbValue for LimitedString<LIMIT> {
    fn to_db_value(&self) -> DbValue {
        self.0.clone().into()
    }
}

impl<const LIMIT: u32> ToDbValue for Option<LimitedString<LIMIT>> {
    fn to_db_value(&self) -> DbValue {
        self.clone().map(|s| s.0).into()
    }
}

impl<T: Model + Send + Sync> DatabaseField for ForeignKey<T> {
    const NULLABLE: bool = T::PrimaryKey::NULLABLE;
    const TYPE: ColumnType = T::PrimaryKey::TYPE;
}

impl<T: Model + Send + Sync> FromDbValue for ForeignKey<T> {
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self> {
        T::PrimaryKey::from_sqlite(value).map(ForeignKey::PrimaryKey)
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
        T::PrimaryKey::from_postgres(value).map(ForeignKey::PrimaryKey)
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
        T::PrimaryKey::from_mysql(value).map(ForeignKey::PrimaryKey)
    }
}

impl<T: Model + Send + Sync> ToDbFieldValue for ForeignKey<T> {
    fn to_db_field_value(&self) -> DbFieldValue {
        self.primary_key().to_db_field_value()
    }
}

impl<T: Model + Send + Sync> FromDbValue for Option<ForeignKey<T>>
where
    Option<T::PrimaryKey>: FromDbValue,
{
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self> {
        Ok(<Option<T::PrimaryKey>>::from_sqlite(value)?.map(ForeignKey::PrimaryKey))
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self> {
        Ok(<Option<T::PrimaryKey>>::from_postgres(value)?.map(ForeignKey::PrimaryKey))
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self> {
        Ok(<Option<T::PrimaryKey>>::from_mysql(value)?.map(ForeignKey::PrimaryKey))
    }
}

impl<T: Model + Send + Sync> ToDbFieldValue for Option<ForeignKey<T>>
where
    Option<T::PrimaryKey>: ToDbFieldValue,
{
    fn to_db_field_value(&self) -> DbFieldValue {
        match self {
            Some(foreign_key) => foreign_key.to_db_field_value(),
            None => <Option<T::PrimaryKey>>::None.to_db_field_value(),
        }
    }
}

impl<T: DatabaseField> DatabaseField for Auto<T> {
    const NULLABLE: bool = T::NULLABLE;
    const TYPE: ColumnType = T::TYPE;
}

impl<T: DatabaseField> FromDbValue for Auto<T> {
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self::fixed(T::from_sqlite(value)?))
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self::fixed(T::from_postgres(value)?))
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self::fixed(T::from_mysql(value)?))
    }
}

impl<T: DatabaseField> ToDbFieldValue for Auto<T> {
    fn to_db_field_value(&self) -> DbFieldValue {
        match self {
            Self::Fixed(value) => value.to_db_field_value(),
            Self::Auto => DbFieldValue::Auto,
        }
    }
}

impl<T: DatabaseField> FromDbValue for Option<Auto<T>>
where
    Option<T>: FromDbValue,
{
    #[cfg(feature = "sqlite")]
    fn from_sqlite(value: SqliteValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        <Option<T>>::from_sqlite(value).map(|value| value.map(Auto::fixed))
    }

    #[cfg(feature = "postgres")]
    fn from_postgres(value: PostgresValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        <Option<T>>::from_postgres(value).map(|value| value.map(Auto::fixed))
    }

    #[cfg(feature = "mysql")]
    fn from_mysql(value: MySqlValueRef<'_>) -> Result<Self>
    where
        Self: Sized,
    {
        <Option<T>>::from_mysql(value).map(|value| value.map(Auto::fixed))
    }
}

impl<T: DatabaseField> ToDbFieldValue for Option<Auto<T>>
where
    Option<T>: ToDbFieldValue,
{
    fn to_db_field_value(&self) -> DbFieldValue {
        match self {
            Some(auto) => auto.to_db_field_value(),
            None => <Option<T>>::None.to_db_field_value(),
        }
    }
}

impl<T: PrimaryKey> PrimaryKey for Auto<T> {}

impl PrimaryKey for i32 {}

impl PrimaryKey for i64 {}
