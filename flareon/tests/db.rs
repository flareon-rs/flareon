#![cfg(feature = "fake")]

use fake::rand::rngs::StdRng;
use fake::rand::SeedableRng;
use fake::{Dummy, Fake, Faker};
use flareon::db::migrations::{Field, Operation};
use flareon::db::query::ExprEq;
use flareon::db::{model, query, Database, DatabaseField, Identifier, LimitedString, Model};
use flareon::test::TestDatabase;

#[flareon_macros::dbtest]
async fn model_crud(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);

    let mut model = TestModel {
        id: 0,
        name: "test".to_owned(),
    };
    model.save(&**test_db).await.unwrap();
    let objects = TestModel::objects().all(&**test_db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test");

    TestModel::objects()
        .filter(<TestModel as Model>::Fields::id.eq(1))
        .delete(&**test_db)
        .await
        .unwrap();

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);
}

#[flareon_macros::dbtest]
async fn model_macro_filtering(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);

    let mut model = TestModel {
        id: 0,
        name: "test".to_owned(),
    };
    model.save(&**test_db).await.unwrap();
    let objects = query!(TestModel, $name == "test")
        .all(&**test_db)
        .await
        .unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test");

    let objects = query!(TestModel, $name == "t")
        .all(&**test_db)
        .await
        .unwrap();
    assert!(objects.is_empty());
}

#[derive(Debug, PartialEq)]
#[model]
struct TestModel {
    id: i32,
    name: String,
}

async fn migrate_test_model(db: &Database) {
    crate::CREATE_TEST_MODEL.forwards(db).await.unwrap();
}

const CREATE_TEST_MODEL: Operation = Operation::create_model()
    .table_name(Identifier::new("test_model"))
    .fields(&[
        Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
            .primary_key()
            .auto(),
        Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
    ])
    .build();

macro_rules! all_fields_migration_field {
    ($name:ident, $ty:ty) => {
        Field::new(
            Identifier::new(concat!("field_", stringify!($name))),
            <$ty as DatabaseField>::TYPE,
        )
        .set_null(<$ty as DatabaseField>::NULLABLE)
    };
    ($ty:ty) => {
        Field::new(
            Identifier::new(concat!("field_", stringify!($ty))),
            <$ty as DatabaseField>::TYPE,
        )
        .set_null(<$ty as DatabaseField>::NULLABLE)
    };
}

#[derive(Debug, PartialEq, Dummy)]
#[model]
struct AllFieldsModel {
    #[dummy(expr = "0i32")]
    id: i32,
    field_bool: bool,
    field_i8: i8,
    field_i16: i16,
    field_i32: i32,
    field_i64: i64,
    field_u8: u8,
    field_u16: u16,
    field_u32: u32,
    // SQLite only allows us to store signed integers, so we're generating numbers that do not
    // exceed i64::MAX
    #[dummy(faker = "0..i64::MAX as u64")]
    field_u64: u64,
    field_f32: f32,
    field_f64: f64,
    field_date: chrono::NaiveDate,
    field_time: chrono::NaiveTime,
    field_datetime: chrono::NaiveDateTime,
    #[dummy(faker = "fake::chrono::Precision::<6>")]
    field_datetime_timezone: chrono::DateTime<chrono::FixedOffset>,
    field_string: String,
    field_blob: Vec<u8>,
    field_option: Option<String>,
    field_limited_string: LimitedString<10>,
}

async fn migrate_all_fields_model(db: &Database) {
    CREATE_ALL_FIELDS_MODEL.forwards(db).await.unwrap();
}

const CREATE_ALL_FIELDS_MODEL: Operation = Operation::create_model()
    .table_name(Identifier::new("all_fields_model"))
    .fields(&[
        Field::new(Identifier::new("id"), <i32 as DatabaseField>::TYPE)
            .primary_key()
            .auto(),
        all_fields_migration_field!(bool),
        all_fields_migration_field!(i8),
        all_fields_migration_field!(i16),
        all_fields_migration_field!(i32),
        all_fields_migration_field!(i64),
        all_fields_migration_field!(u8),
        all_fields_migration_field!(u16),
        all_fields_migration_field!(u32),
        all_fields_migration_field!(u64),
        all_fields_migration_field!(f32),
        all_fields_migration_field!(f64),
        all_fields_migration_field!(date, chrono::NaiveDate),
        all_fields_migration_field!(time, chrono::NaiveTime),
        all_fields_migration_field!(datetime, chrono::NaiveDateTime),
        all_fields_migration_field!(datetime_timezone, chrono::DateTime<chrono::FixedOffset>),
        all_fields_migration_field!(string, String),
        all_fields_migration_field!(blob, Vec<u8>),
        all_fields_migration_field!(option, Option<String>),
        all_fields_migration_field!(limited_string, LimitedString<10>),
    ])
    .build();

#[flareon_macros::dbtest]
async fn all_fields_model(db: &mut TestDatabase) {
    migrate_all_fields_model(&db).await;

    assert_eq!(AllFieldsModel::objects().all(&**db).await.unwrap(), vec![]);

    let r = &mut StdRng::seed_from_u64(123_785);
    let mut models = (0..100)
        .map(|_| Faker.fake_with_rng(r))
        .collect::<Vec<AllFieldsModel>>();
    for model in &mut models {
        model.save(&**db).await.unwrap();
    }

    let mut models_from_db: Vec<_> = AllFieldsModel::objects().all(&**db).await.unwrap();
    models_from_db.iter_mut().for_each(|model| model.id = 0);
    normalize_datetimes(&mut models);
    normalize_datetimes(&mut models_from_db);

    assert_eq!(models.len(), models_from_db.len());
    assert!(models.iter().all(|model| models_from_db.contains(model)));
}

/// Normalize the datetimes to UTC.
fn normalize_datetimes(data: &mut Vec<AllFieldsModel>) {
    for model in data {
        model.field_datetime_timezone = model.field_datetime_timezone.with_timezone(
            &chrono::FixedOffset::east_opt(0).expect("UTC timezone is always valid"),
        );
    }
}
