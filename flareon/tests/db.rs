use fake::{Dummy, Fake, Faker};
use flareon::db::migrations::{Field, Operation};
use flareon::db::query::ExprEq;
use flareon::db::{model, Database, DbField, Identifier, Model};
use rand::rngs::StdRng;
use rand::SeedableRng;

#[tokio::test]
async fn test_model_crud() {
    let db = test_sqlite_db().await;

    migrate_test_model(&db).await;

    assert_eq!(TestModel::objects().all(&db).await.unwrap(), vec![]);

    let mut model = TestModel {
        id: 0,
        name: "test".to_owned(),
    };
    model.save(&db).await.unwrap();
    let objects = TestModel::objects().all(&db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test");

    TestModel::objects()
        .filter(<TestModel as Model>::Fields::id.eq(1))
        .delete(&db)
        .await
        .unwrap();

    assert_eq!(TestModel::objects().all(&db).await.unwrap(), vec![]);

    db.close().await.unwrap();
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

const CREATE_TEST_MODEL: Operation = Operation::CreateModel {
    table_name: Identifier::new("test_model"),
    fields: &[
        Field::new(Identifier::new("id"), <i32 as DbField>::TYPE)
            .primary_key()
            .auto(),
        Field::new(Identifier::new("name"), <String as DbField>::TYPE),
    ],
};

macro_rules! all_fields_migration_field {
    ($name:ident, $ty:ty) => {
        Field::new(
            Identifier::new(concat!("field_", stringify!($name))),
            <$ty as DbField>::TYPE,
        )
    };
    ($ty:ty) => {
        Field::new(
            Identifier::new(concat!("field_", stringify!($ty))),
            <$ty as DbField>::TYPE,
        )
    };
}

#[derive(Debug, PartialEq, Dummy)]
#[model]
struct AllFieldsModel {
    #[dummy(expr = "0i32")]
    id: i32,
    field_i16: i16,
    field_i32: i32,
    field_i64: i64,
    field_string: String,
}

async fn migrate_all_fields_model(db: &Database) {
    CREATE_ALL_FIELDS_MODEL.forwards(db).await.unwrap();
}

const CREATE_ALL_FIELDS_MODEL: Operation = Operation::CreateModel {
    table_name: Identifier::new("all_fields_model"),
    fields: &[
        Field::new(Identifier::new("id"), <i32 as DbField>::TYPE)
            .primary_key()
            .auto(),
        all_fields_migration_field!(i16),
        all_fields_migration_field!(i32),
        all_fields_migration_field!(i64),
        all_fields_migration_field!(string, String),
    ],
};

#[tokio::test]
async fn test_all_fields_model() {
    let db = test_sqlite_db().await;

    migrate_all_fields_model(&db).await;

    assert_eq!(AllFieldsModel::objects().all(&db).await.unwrap(), vec![]);

    let r = &mut StdRng::seed_from_u64(123_785);
    for _ in 0..100 {
        let mut model: AllFieldsModel = Faker.fake_with_rng(r);
        model.save(&db).await.unwrap();
    }

    let objects = AllFieldsModel::objects().all(&db).await.unwrap();
    assert_eq!(objects.len(), 100);

    db.close().await.unwrap();
}

async fn test_sqlite_db() -> Database {
    Database::new("sqlite::memory:").await.unwrap()
}
