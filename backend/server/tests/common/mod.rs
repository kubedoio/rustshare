use rustshare_auth::JwtManager;
use rustshare_server::AppState;
use uuid::Uuid;

pub async fn setup_test_server() -> (AppState, String) {
    // Initialize test database, services, start server
    // Return (state, base_url)
    todo!("Implement test server setup")
}

pub fn create_test_token(state: &AppState) -> String {
    let user_id = Uuid::new_v4();
    state
        .jwt_manager
        .generate(user_id, "test@example.com".to_string(), Uuid::nil())
        .unwrap()
}

pub fn get_user_id_from_token(token: &str, state: &AppState) -> Uuid {
    let claims = state.jwt_manager.validate(token).unwrap();
    Uuid::parse_str(&claims.sub).unwrap()
}

pub fn create_test_file_upload(filename: &str, content: &[u8]) -> reqwest::multipart::Form {
    reqwest::multipart::Form::new()
        .text("name", filename.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(content.to_vec()).file_name(filename.to_string()),
        )
}
