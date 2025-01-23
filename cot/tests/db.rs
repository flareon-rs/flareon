#![cfg(feature = "fake")]
#![cfg_attr(miri, ignore)]

use cot::db::migrations::{Field, Operation};
use cot::db::query::ExprEq;
use cot::db::{
    model, query, Auto, Database, DatabaseError, DatabaseField, ForeignKey,
    ForeignKeyOnDeletePolicy, ForeignKeyOnUpdatePolicy, Identifier, LimitedString, Model,
};
use cot::test::TestDatabase;
use fake::rand::rngs::StdRng;
use fake::rand::SeedableRng;
use fake::{Dummy, Fake, Faker};

#[cot_macros::dbtest]
async fn model_crud(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);

    // Create
    let mut model = TestModel {
        id: Auto::fixed(1),
        name: "test".to_owned(),
    };
    model.save(&**test_db).await.unwrap();

    // Read
    let objects = TestModel::objects().all(&**test_db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test");

    // Update (& read again)
    model.name = "test2".to_owned();
    model.save(&**test_db).await.unwrap();
    let objects = TestModel::objects().all(&**test_db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test2");

    // Delete
    TestModel::objects()
        .filter(<TestModel as Model>::Fields::id.eq(1))
        .delete(&**test_db)
        .await
        .unwrap();

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);
}

#[cot_macros::dbtest]
async fn model_insert(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    // Insert
    let mut model = TestModel {
        id: Auto::fixed(1),
        name: "test".to_owned(),
    };
    let result = model.insert(&**test_db).await;
    assert!(result.is_ok());

    // Can't insert the same model instance again
    let result = model.insert(&**test_db).await;
    assert!(result.is_err());

    // Read the model from the database
    let objects = TestModel::objects().all(&**test_db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test");
}

#[cot_macros::dbtest]
async fn model_update(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    // Insert
    let mut model = TestModel {
        id: Auto::fixed(1),
        name: "test".to_owned(),
    };
    let result = model.insert(&**test_db).await;
    assert!(result.is_ok());

    // Update
    model.name = "test2".to_owned();
    let result = model.update(&**test_db).await;
    assert!(result.is_ok());

    // Can't update non-existing object
    let mut model = TestModel {
        id: Auto::fixed(2),
        name: "test3".to_owned(),
    };
    let result = model.update(&**test_db).await;
    assert!(result.is_err());

    // Read the model from the database
    let objects = TestModel::objects().all(&**test_db).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "test2");
}

#[cot_macros::dbtest]
async fn model_macro_filtering(test_db: &mut TestDatabase) {
    migrate_test_model(&*test_db).await;

    assert_eq!(TestModel::objects().all(&**test_db).await.unwrap(), vec![]);

    let mut model = TestModel {
        id: Auto::auto(),
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
    id: Auto<i32>,
    name: String,
}

async fn migrate_test_model(db: &Database) {
    crate::CREATE_TEST_MODEL.forwards(db).await.unwrap();
}

const CREATE_TEST_MODEL: Operation = Operation::create_model()
    .table_name(Identifier::new("test_model"))
    .fields(&[
        Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
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
    #[dummy(expr = "Auto::auto()")]
    id: Auto<i32>,
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
    #[dummy(faker = "fake::chrono::Precision::<6>")]
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
        Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
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

#[cot_macros::dbtest]
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
    normalize_datetimes(&mut models);
    normalize_datetimes(&mut models_from_db);

    assert_eq!(models.len(), models_from_db.len());
    for model in &models {
        assert!(
            models_from_db.contains(model),
            "Could not find model {:?} in models_from_db: {:?}",
            model,
            models_from_db
        );
    }
}

/// Normalize the datetimes to UTC.
fn normalize_datetimes(data: &mut Vec<AllFieldsModel>) {
    for model in data {
        model.field_datetime_timezone = model.field_datetime_timezone.with_timezone(
            &chrono::FixedOffset::east_opt(0).expect("UTC timezone is always valid"),
        );
    }
}

#[cot_macros::dbtest]
async fn foreign_keys(db: &mut TestDatabase) {
    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Artist {
        id: Auto<i32>,
        name: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Track {
        id: Auto<i32>,
        artist: ForeignKey<Artist>,
        name: String,
    }

    const CREATE_ARTIST: Operation = Operation::create_model()
        .table_name(Identifier::new("artist"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
            Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
        ])
        .build();
    const CREATE_TRACK: Operation = Operation::create_model()
        .table_name(Identifier::new("track"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
            Field::new(
                Identifier::new("artist"),
                <ForeignKey<Artist> as DatabaseField>::TYPE,
            )
            .foreign_key(
                <Artist as Model>::TABLE_NAME,
                <Artist as Model>::PRIMARY_KEY_NAME,
                ForeignKeyOnDeletePolicy::Restrict,
                ForeignKeyOnUpdatePolicy::Restrict,
            ),
            Field::new(Identifier::new("name"), <String as DatabaseField>::TYPE),
        ])
        .build();

    CREATE_ARTIST.forwards(db).await.unwrap();
    CREATE_TRACK.forwards(db).await.unwrap();

    let mut artist = Artist {
        id: Auto::auto(),
        name: "artist".to_owned(),
    };
    artist.save(&**db).await.unwrap();

    let mut track = Track {
        id: Auto::auto(),
        artist: ForeignKey::from(&artist),
        name: "track".to_owned(),
    };
    track.save(&**db).await.unwrap();

    let mut track = Track::objects().all(&**db).await.unwrap()[0].clone();
    let artist_from_db = track.artist.get(&**db).await.unwrap();
    assert_eq!(artist_from_db, &artist);

    let error = query!(Artist, $id == artist.id)
        .delete(&**db)
        .await
        .unwrap_err();
    // expected foreign key violation
    assert!(matches!(error, DatabaseError::DatabaseEngineError(_)));

    query!(Track, $artist == &artist)
        .delete(&**db)
        .await
        .unwrap();
    query!(Artist, $id == artist.id)
        .delete(&**db)
        .await
        .unwrap();
    // no error should be thrown
}

#[cot_macros::dbtest]
async fn foreign_keys_option(db: &mut TestDatabase) {
    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Parent {
        id: Auto<i32>,
    }

    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Child {
        id: Auto<i32>,
        parent: Option<ForeignKey<Parent>>,
    }

    const CREATE_PARENT: Operation = Operation::create_model()
        .table_name(Identifier::new("parent"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
        ])
        .build();
    const CREATE_CHILD: Operation = Operation::create_model()
        .table_name(Identifier::new("child"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
            Field::new(
                Identifier::new("parent"),
                <Option<ForeignKey<Parent>> as DatabaseField>::TYPE,
            )
            .set_null(<Option<ForeignKey<Parent>> as DatabaseField>::NULLABLE)
            .foreign_key(
                <Parent as Model>::TABLE_NAME,
                <Parent as Model>::PRIMARY_KEY_NAME,
                ForeignKeyOnDeletePolicy::SetNone,
                ForeignKeyOnUpdatePolicy::SetNone,
            ),
        ])
        .build();

    CREATE_PARENT.forwards(db).await.unwrap();
    CREATE_CHILD.forwards(db).await.unwrap();

    // Test child with `None` parent
    let mut child = Child {
        id: Auto::auto(),
        parent: None,
    };
    child.save(&**db).await.unwrap();

    let child = Child::objects().all(&**db).await.unwrap()[0].clone();
    assert_eq!(child.parent, None);

    query!(Child, $id == child.id).delete(&**db).await.unwrap();

    // Test child with `Some` parent
    let mut parent = Parent { id: Auto::auto() };
    parent.save(&**db).await.unwrap();

    let mut child = Child {
        id: Auto::auto(),
        parent: Some(ForeignKey::from(&parent)),
    };
    child.save(&**db).await.unwrap();

    let child = Child::objects().all(&**db).await.unwrap()[0].clone();
    let mut parent_fk = child.parent.unwrap();
    let parent_from_db = parent_fk.get(&**db).await.unwrap();
    assert_eq!(parent_from_db, &parent);

    // Check none policy
    query!(Parent, $id == parent.id)
        .delete(&**db)
        .await
        .unwrap();
    let child = Child::objects().all(&**db).await.unwrap()[0].clone();
    assert_eq!(child.parent, None);
}

#[cot_macros::dbtest]
async fn foreign_keys_cascade(db: &mut TestDatabase) {
    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Parent {
        id: Auto<i32>,
    }

    #[derive(Debug, Clone, PartialEq)]
    #[model]
    struct Child {
        id: Auto<i32>,
        parent: Option<ForeignKey<Parent>>,
    }

    const CREATE_PARENT: Operation = Operation::create_model()
        .table_name(Identifier::new("parent"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
        ])
        .build();
    const CREATE_CHILD: Operation = Operation::create_model()
        .table_name(Identifier::new("child"))
        .fields(&[
            Field::new(Identifier::new("id"), <Auto<i32> as DatabaseField>::TYPE)
                .primary_key()
                .auto(),
            Field::new(
                Identifier::new("parent"),
                <Option<ForeignKey<Parent>> as DatabaseField>::TYPE,
            )
            .set_null(<Option<ForeignKey<Parent>> as DatabaseField>::NULLABLE)
            .foreign_key(
                <Parent as Model>::TABLE_NAME,
                <Parent as Model>::PRIMARY_KEY_NAME,
                ForeignKeyOnDeletePolicy::Cascade,
                ForeignKeyOnUpdatePolicy::Cascade,
            ),
        ])
        .build();

    CREATE_PARENT.forwards(db).await.unwrap();
    CREATE_CHILD.forwards(db).await.unwrap();

    // with parent
    let mut parent = Parent { id: Auto::auto() };
    parent.save(&**db).await.unwrap();

    let mut child = Child {
        id: Auto::auto(),
        parent: Some(ForeignKey::from(&parent)),
    };
    child.save(&**db).await.unwrap();

    let child = Child::objects().all(&**db).await.unwrap()[0].clone();
    let mut parent_fk = child.parent.unwrap();
    let parent_from_db = parent_fk.get(&**db).await.unwrap();
    assert_eq!(parent_from_db, &parent);

    // Check cascade policy
    query!(Parent, $id == parent.id)
        .delete(&**db)
        .await
        .unwrap();
    assert!(Child::objects().all(&**db).await.unwrap().is_empty());
}
