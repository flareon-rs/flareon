use std::sync::Arc;

use flareon::auth::db::{DatabaseUser, DatabaseUserCredentials};
use flareon::auth::{AuthRequestExt, Password};
use flareon::test::{TestDatabaseBuilder, TestRequestBuilder};

#[tokio::test]
async fn database_user() {
    let db = Arc::new(TestDatabaseBuilder::new().with_auth().build().await);
    let mut request_builder = TestRequestBuilder::get("/");
    request_builder.with_db_auth(db.clone());

    // Anonymous user
    let mut request = request_builder.clone().with_session().build();
    let user = request.user().await.unwrap();
    assert_eq!(user.is_authenticated(), false);

    // Authenticated user
    DatabaseUser::create_user(&*db, "testuser".to_string(), &Password::new("password123"))
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
    assert_eq!(user.is_authenticated(), true);
    assert_eq!(user.username(), Some("testuser"));

    // Log in
    request.login(user).await.unwrap();
    let user = request.user().await.unwrap();
    assert_eq!(user.is_authenticated(), true);
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
    assert_eq!(user.is_authenticated(), true);
    assert_eq!(user.username(), Some("testuser"));

    // Log out
    request.logout().await.unwrap();
    let user = request.user().await.unwrap();
    assert_eq!(user.is_authenticated(), false);
}
