use cot::auth::db::{DatabaseUser, DatabaseUserCredentials};
use cot::auth::{AuthRequestExt, Password};
use cot::test::{TestDatabase, TestRequestBuilder};

#[cot_macros::dbtest]
async fn database_user(test_db: &mut TestDatabase) {
    test_db.with_auth().run_migrations().await;
    let mut request_builder = TestRequestBuilder::get("/");
    request_builder.with_db_auth(test_db.database());

    // Anonymous user
    let mut request = request_builder.clone().with_session().build();
    let user = request.user().await.unwrap();
    assert!(!user.is_authenticated());

    // Authenticated user
    DatabaseUser::create_user(
        &**test_db,
        "testuser".to_string(),
        &Password::new("password123"),
    )
    .await
    .unwrap();

    let user = request
        .authenticate(&DatabaseUserCredentials::new(
            "testuser".to_string(),
            Password::new("password123"),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(user.is_authenticated());
    assert_eq!(user.username(), Some("testuser"));

    // Log in
    request.login(user).await.unwrap();
    let user = request.user().await.unwrap();
    assert!(user.is_authenticated());
    assert_eq!(user.username(), Some("testuser"));

    // Invalid credentials
    let user = request
        .authenticate(&DatabaseUserCredentials::new(
            "testuser".to_string(),
            Password::new("wrongpassword"),
        ))
        .await
        .unwrap();
    assert!(user.is_none());

    // User persists between requests
    let mut request = request_builder.clone().with_session_from(&request).build();
    let user = request.user().await.unwrap();
    assert!(user.is_authenticated());
    assert_eq!(user.username(), Some("testuser"));

    // Log out
    request.logout().await.unwrap();
    let user = request.user().await.unwrap();
    assert!(!user.is_authenticated());
}
