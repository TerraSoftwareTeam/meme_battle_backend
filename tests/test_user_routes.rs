// use axum::http::{Method, StatusCode};

// use meme_battle_backend::{
//     common::{http::dto::RestApiResponse, http::error::AppError},
//     domains::user::dto::user_dto::{CreateUserDto, SearchUserDto, UpdateUserDto, UserDto},
// };

// mod test_helpers;

// use test_helpers::{deserialize_json_body, request_with_auth, request_with_auth_and_body};

// async fn create_user() -> Result<(CreateUserDto, UserDto), AppError> {
//     let username = format!("testuser-{}", uuid::Uuid::new_v4());
//     let handle = format!("handle-{}", uuid::Uuid::new_v4());

//     let payload = CreateUserDto { username, handle };

//     let response = request_with_auth_and_body(Method::POST, "/user", &payload);
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<UserDto> = deserialize_json_body(body).await.unwrap();

//     assert_eq!(response_body.0.status, StatusCode::OK);
//     let user_dto = response_body.0.data.unwrap();

//     Ok((payload, user_dto))
// }

// #[tokio::test]
// async fn test_create_user() {
//     let created = create_user().await.expect("Failed to create user");

//     let payload = created.0;
//     let user_dto = created.1;

//     assert!(!user_dto.id.is_empty());
//     assert_eq!(user_dto.username, payload.username);
//     assert_eq!(user_dto.handle, payload.handle);
// }

// #[tokio::test]
// async fn test_get_users() {
//     let response = request_with_auth(Method::GET, "/user");
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<Vec<UserDto>> = deserialize_json_body(body).await.unwrap();

//     assert_eq!(response_body.0.status, StatusCode::OK);
//     assert!(!response_body.0.data.unwrap().is_empty());
// }

// #[tokio::test]
// async fn test_get_user_list() {
//     let payload = SearchUserDto {
//         username: Some("User".to_string()),
//         id: None,
//         handle: None,
//     };

//     let response = request_with_auth_and_body(Method::POST, "/user/list", &payload);
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<Vec<UserDto>> = deserialize_json_body(body).await.unwrap();

//     assert_eq!(response_body.0.status, StatusCode::OK);
//     assert!(!response_body.0.data.unwrap().is_empty());
// }

// #[tokio::test]
// async fn test_get_user_by_id() {
//     let created = create_user().await.expect("Failed to create user");

//     let existent_user = created.1;
//     let existent_id = existent_user.id.clone();

//     let url = format!("/user/{}", existent_id);
//     let response = request_with_auth(Method::GET, url.as_str());
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<UserDto> = deserialize_json_body(body).await.unwrap();

//     assert_eq!(response_body.0.status, StatusCode::OK);
//     let user_dto = response_body.0.data.unwrap();

//     assert_eq!(user_dto.id, existent_id);
//     assert_eq!(user_dto.username, existent_user.username);
//     assert_eq!(user_dto.handle, existent_user.handle);
//     assert_eq!(user_dto.created_at, existent_user.created_at);
//     assert_eq!(user_dto.modified_at, existent_user.modified_at);
// }

// #[tokio::test]
// async fn test_update_user() {
//     let created = create_user().await.expect("Failed to create user");

//     let existent_id = created.1.id;
//     let username = format!("update-testuser-{}", uuid::Uuid::new_v4());
//     let handle = format!("update-handle-{}", uuid::Uuid::new_v4());

//     let payload = UpdateUserDto { username, handle };

//     let url = format!("/user/{}", existent_id);
//     let response = request_with_auth_and_body(Method::PUT, url.as_str(), &payload);
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<UserDto> = deserialize_json_body(body).await.unwrap();

//     assert_eq!(response_body.0.status, StatusCode::OK);
//     let user_dto = response_body.0.data.unwrap();

//     assert_eq!(user_dto.id, existent_id);
//     assert_eq!(user_dto.username, payload.username);
//     assert_eq!(user_dto.handle, payload.handle);
// }

// #[tokio::test]
// async fn test_delete_user_not_found() {
//     let non_existent_id = uuid::Uuid::new_v4();

//     let url = format!("/user/{}", non_existent_id);
//     let response = request_with_auth(Method::DELETE, url.as_str());
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::NOT_FOUND);

//     let response_body: RestApiResponse<()> = deserialize_json_body(body).await.unwrap();
//     assert_eq!(response_body.0.status, StatusCode::NOT_FOUND);
// }

// #[tokio::test]
// async fn test_delete_user() {
//     let created = create_user()
//         .await
//         .expect("Failed to create user for deletion");

//     let url = format!("/user/{}", created.1.id);
//     let response = request_with_auth(Method::DELETE, url.as_str());
//     let (parts, body) = response.await.into_parts();

//     assert_eq!(parts.status, StatusCode::OK);

//     let response_body: RestApiResponse<()> = deserialize_json_body(body).await.unwrap();
//     assert_eq!(response_body.0.status, StatusCode::OK);
// }
