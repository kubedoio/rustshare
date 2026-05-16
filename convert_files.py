import re

path = "backend/server/src/handlers/files.rs"
with open(path, "r") as f:
    content = f.read()

# 1. Imports
content = content.replace(
    "use super::{file_error_response, AuthenticatedUser, ErrorResponse};",
    "use super::{AuthenticatedUser, AppError};",
)

# 2. Change return types Result<T, Response> -> Result<T, AppError>
# This also catches Result<Response, Response> -> Result<Response, AppError>
content = re.sub(r"Result<([^,]+),\s*Response>", r"Result<\1, AppError>", content)

# 3. Change bare -> Response for download_file_content and preview_file
content = content.replace(
    """pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Response {""",
    """pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {""",
)

content = content.replace(
    """pub async fn preview_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Response {""",
    """pub async fn preview_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {""",
)

# 4. Remove .map_err(file_error_response)? for service calls
content = content.replace(".map_err(file_error_response)?", "?")

# 5. Replace multipart next_field in upload_file
content = content.replace(
    """    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        file_error_response(FileError::Storage(format!(
            "Failed to read multipart field: {}",
            e
        )))
    })? {""",
    """    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        AppError::internal(format!(
            "Failed to read multipart field: {}",
            e
        ))
    })? {""",
)

# 6. Replace field.bytes() in upload_file
content = content.replace(
    """                file_data = Some(field.bytes().await.map_err(|e| {
                    tracing::error!("Failed to read file data: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read file data: {}",
                        e
                    )))
                })?);""",
    """                file_data = Some(field.bytes().await.map_err(|e| {
                    tracing::error!("Failed to read file data: {}", e);
                    AppError::internal(format!(
                        "Failed to read file data: {}",
                        e
                    ))
                })?);""",
)

# 7. Replace field.text() for name in upload_file
content = content.replace(
    """            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    tracing::error!("Failed to read name field: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read name field: {}",
                        e
                    )))
                })?);
            }""",
    """            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    tracing::error!("Failed to read name field: {}", e);
                    AppError::internal(format!(
                        "Failed to read name field: {}",
                        e
                    ))
                })?);
            }""",
)

# 8. Replace parent_folder_id parsing in upload_file
content = content.replace(
    """            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    tracing::error!("Failed to read parent_folder_id field: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read parent_folder_id field: {}",
                        e
                    )))
                })?;
                parent_folder_id = Some(Uuid::parse_str(&text).map_err(|_| {
                    file_error_response(FileError::InvalidName(
                        "Invalid parent_folder_id".to_string(),
                    ))
                })?);
            }""",
    """            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    tracing::error!("Failed to read parent_folder_id field: {}", e);
                    AppError::internal(format!(
                        "Failed to read parent_folder_id field: {}",
                        e
                    ))
                })?;
                parent_folder_id = Some(Uuid::parse_str(&text).map_err(|_| {
                    AppError::bad_request("Invalid parent_folder_id")
                })?);
            }""",
)

# 9. Replace ok_or_else validations in upload_file
content = content.replace(
    """    let file_data = file_data.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file data".to_string()))
    })?;
    let file_name = file_name.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file name".to_string()))
    })?;

    // Validate file name length and content
    if file_name.trim().is_empty() {
        return Err(file_error_response(FileError::InvalidName(
            "File name must not be empty".to_string(),
        )));
    }
    if file_name.len() > 255 {
        return Err(file_error_response(FileError::InvalidName(
            "File name must not exceed 255 characters".to_string(),
        )));
    }
    if file_name.contains('\0') || file_name.contains('/') {
        return Err(file_error_response(FileError::InvalidName(
            "File name contains invalid characters".to_string(),
        )));
    }""",
    """    let file_data = file_data.ok_or_else(|| {
        AppError::bad_request("Missing file data")
    })?;
    let file_name = file_name.ok_or_else(|| {
        AppError::bad_request("Missing file name")
    })?;

    // Validate file name length and content
    if file_name.trim().is_empty() {
        return Err(AppError::bad_request(
            "File name must not be empty",
        ));
    }
    if file_name.len() > 255 {
        return Err(AppError::bad_request(
            "File name must not exceed 255 characters",
        ));
    }
    if file_name.contains('\0') || file_name.contains('/') {
        return Err(AppError::bad_request(
            "File name contains invalid characters",
        ));
    }""",
)

# 10. Replace hidden kanban file checks
content = content.replace(
    """    if is_hidden_kanban_file(&file.name) {
        return Err(file_error_response(FileError::NotFound(file_id)));
    }""",
    """    if is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }""",
)

# 11. Replace download_file_content body
content = content.replace(
    """pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // Get file metadata first (this also checks permissions)
    let file = match state.file_service.get_file(file_id, auth.user_id).await {
        Ok(file) => file,
        Err(e) => return file_error_response(e).into_response(),
    };

    if is_hidden_kanban_file(&file.name) {
        return file_error_response(FileError::NotFound(file_id)).into_response();
    }

    // Stream the file content directly (avoids redirecting to internal storage URLs)
    let storage_key = file.storage_key();
    let bytes = match state.object_store.get(&storage_key).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to read file content: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to read file content")),
            )
                .into_response();
        }
    };

    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        file.name.replace('"', "\\\""),
        urlencoding::encode(&file.name)
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    (StatusCode::OK, headers, bytes).into_response()
}""",
    """pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // Get file metadata first (this also checks permissions)
    let file = state.file_service.get_file(file_id, auth.user_id).await?;

    if is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    // Stream the file content directly (avoids redirecting to internal storage URLs)
    let storage_key = file.storage_key();
    let bytes = state.object_store.get(&storage_key).await.map_err(|e| {
        tracing::error!("Failed to read file content: {}", e);
        AppError::internal("Failed to read file content")
    })?;

    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        file.name.replace('"', "\\\""),
        urlencoding::encode(&file.name)
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    Ok((StatusCode::OK, headers, bytes).into_response())
}""",
)

# 12. Replace preview_file body
content = content.replace(
    """pub async fn preview_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // Get file metadata first (this also checks permissions)
    let file = match state.file_service.get_file(file_id, auth.user_id).await {
        Ok(file) => file,
        Err(e) => return file_error_response(e).into_response(),
    };

    if is_hidden_kanban_file(&file.name) {
        return file_error_response(FileError::NotFound(file_id)).into_response();
    }

    // Stream the file content directly (avoids redirecting to internal storage URLs)
    let storage_key = file.storage_key();
    let bytes = match state.object_store.get(&storage_key).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to read file content: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to read file content")),
            )
                .into_response();
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );

    (StatusCode::OK, headers, bytes).into_response()
}""",
    """pub async fn preview_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // Get file metadata first (this also checks permissions)
    let file = state.file_service.get_file(file_id, auth.user_id).await?;

    if is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    // Stream the file content directly (avoids redirecting to internal storage URLs)
    let storage_key = file.storage_key();
    let bytes = state.object_store.get(&storage_key).await.map_err(|e| {
        tracing::error!("Failed to read file content: {}", e);
        AppError::internal("Failed to read file content")
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&file.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );

    Ok((StatusCode::OK, headers, bytes).into_response())
}""",
)

# 13. Replace update_file validations and multipart
content = content.replace(
    """    // Parse If-Match header
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            file_error_response(FileError::InvalidName(
                "Missing If-Match header".to_string(),
            ))
        })?;

    let expected_version: i32 = if_match.parse().map_err(|_| {
        file_error_response(FileError::InvalidName(
            "Invalid If-Match header: must be an integer".to_string(),
        ))
    })?;

    // Extract file data from multipart
    let mut file_data: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to read multipart field: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            file_data = Some(field.bytes().await.map_err(|e| {
                file_error_response(FileError::Storage(format!(
                    "Failed to read file data: {}",
                    e
                )))
            })?);
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file data".to_string()))
    })?;""",
    """    // Parse If-Match header
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            AppError::bad_request("Missing If-Match header")
        })?;

    let expected_version: i32 = if_match.parse().map_err(|_| {
        AppError::bad_request("Invalid If-Match header: must be an integer")
    })?;

    // Extract file data from multipart
    let mut file_data: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::internal(format!(
            "Failed to read multipart field: {}",
            e
        ))
    })? {
        if field.name() == Some("file") {
            file_data = Some(field.bytes().await.map_err(|e| {
                AppError::internal(format!(
                    "Failed to read file data: {}",
                    e
                ))
            })?);
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| {
        AppError::bad_request("Missing file data")
    })?;""",
)

# 14. Replace get_file_thumbnail
content = content.replace(
    """pub async fn get_file_thumbnail(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<ThumbnailParams>,
) -> Result<Response, AppError> {
    // First, verify the user has access to the file
    let file = state
        .file_service
        .get_file(file_id, user_id)
        .await
        .map_err(file_error_response)?;

    // Check file size - don't generate thumbnails for files larger than 100MB
    if file.size > MAX_THUMBNAIL_FILE_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: "File too large for thumbnail generation".to_string(),
                details: Some(format!(
                    "File size {} exceeds maximum allowed {} bytes",
                    file.size, MAX_THUMBNAIL_FILE_SIZE
                )),
            }),
        )
            .into_response());
    }

    // Parse size parameter (default to "md")
    let size_str = params.size.as_deref().unwrap_or("md");
    let size = ThumbnailSize::try_from(size_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "Invalid size parameter: {}. Use 'sm', 'md', or 'lg'",
                size_str
            ))),
        )
            .into_response()
    })?;

    // Check if thumbnail exists
    let thumbnail = match state.thumbnail_service.get_thumbnail(file_id, size).await {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => {
            // Thumbnail doesn't exist, try to generate it
            state
                .thumbnail_service
                .generate_thumbnail(file_id, &file.mime_type, &file.name, size)
                .await
                .map_err(thumbnail_error_response)?
        }
        Err(e) => {
            tracing::error!("Failed to get thumbnail: {}", e);
            return Err(thumbnail_error_response(e));
        }
    };

    // Get thumbnail data from storage
    let thumbnail_data = state
        .thumbnail_service
        .get_thumbnail_data(&thumbnail.storage_path)
        .await
        .map_err(thumbnail_error_response)?;

    // Build response with cache headers
    // Thumbnails are immutable once generated
    let etag = format!("{}-{}", file_id, size_str);
    let headers = [
        (header::CONTENT_TYPE, thumbnail.content_type.as_str()),
        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        (header::ETAG, etag.as_str()),
    ];

    let response = (StatusCode::OK, headers, thumbnail_data).into_response();

    Ok(response)
}""",
    """pub async fn get_file_thumbnail(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<ThumbnailParams>,
) -> Result<Response, AppError> {
    // First, verify the user has access to the file
    let file = state
        .file_service
        .get_file(file_id, user_id)
        .await?;

    // Check file size - don't generate thumbnails for files larger than 100MB
    if file.size > MAX_THUMBNAIL_FILE_SIZE {
        return Err(AppError::payload_too_large(format!(
            "File size {} exceeds maximum allowed {} bytes",
            file.size, MAX_THUMBNAIL_FILE_SIZE
        )));
    }

    // Parse size parameter (default to "md")
    let size_str = params.size.as_deref().unwrap_or("md");
    let size = ThumbnailSize::try_from(size_str).map_err(|_| {
        AppError::bad_request(format!(
            "Invalid size parameter: {}. Use 'sm', 'md', or 'lg'",
            size_str
        ))
    })?;

    // Check if thumbnail exists
    let thumbnail = match state.thumbnail_service.get_thumbnail(file_id, size).await {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => {
            // Thumbnail doesn't exist, try to generate it
            state
                .thumbnail_service
                .generate_thumbnail(file_id, &file.mime_type, &file.name, size)
                .await
                .map_err(|e| match e {
                    ThumbnailError::NotFound => AppError::not_found("File not found"),
                    ThumbnailError::UnsupportedType => AppError::unsupported_media_type("Thumbnail generation not supported for this file type"),
                    _ => {
                        tracing::error!("Thumbnail service error: {}", e);
                        AppError::internal("Failed to generate thumbnail")
                    }
                })?
        }
        Err(e) => {
            tracing::error!("Failed to get thumbnail: {}", e);
            return Err(match e {
                ThumbnailError::NotFound => AppError::not_found("File not found"),
                ThumbnailError::UnsupportedType => AppError::unsupported_media_type("Thumbnail generation not supported for this file type"),
                _ => {
                    tracing::error!("Thumbnail service error: {}", e);
                    AppError::internal("Failed to generate thumbnail")
                }
            });
        }
    };

    // Get thumbnail data from storage
    let thumbnail_data = state
        .thumbnail_service
        .get_thumbnail_data(&thumbnail.storage_path)
        .await
        .map_err(|e| match e {
            ThumbnailError::NotFound => AppError::not_found("File not found"),
            ThumbnailError::UnsupportedType => AppError::unsupported_media_type("Thumbnail generation not supported for this file type"),
            _ => {
                tracing::error!("Thumbnail service error: {}", e);
                AppError::internal("Failed to generate thumbnail")
            }
        })?;

    // Build response with cache headers
    // Thumbnails are immutable once generated
    let etag = format!("{}-{}", file_id, size_str);
    let headers = [
        (header::CONTENT_TYPE, thumbnail.content_type.as_str()),
        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        (header::ETAG, etag.as_str()),
    ];

    let response = (StatusCode::OK, headers, thumbnail_data).into_response();

    Ok(response)
}""",
)

# 15. Remove thumbnail_error_response function definition
content = content.replace(
    """/// Map ThumbnailError to HTTP response.
fn thumbnail_error_response(err: ThumbnailError) -> Response {
    let (status, message) = match err {
        ThumbnailError::NotFound => (StatusCode::NOT_FOUND, "File not found".to_string()),
        ThumbnailError::UnsupportedType => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Thumbnail generation not supported for this file type".to_string(),
        ),
        ThumbnailError::Storage(_)
        | ThumbnailError::Generation(_)
        | ThumbnailError::Database(_) => {
            tracing::error!("Thumbnail service error: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate thumbnail".to_string(),
            )
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}

""",
    "",
)

# 16. Replace edit_file
content = content.replace(
    """pub async fn edit_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<EditFileRequest>,
) -> Result<Json<EditFileResponse>, AppError> {
    // Validate save mode
    if req.save_mode != "overwrite" && req.save_mode != "new_version" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Invalid save_mode. Must be 'overwrite' or 'new_version'",
            )),
        )
            .into_response());
    }

    // Decode base64 content
    let content =
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.content) {
            Ok(bytes) => Bytes::from(bytes),
            Err(e) => {
                tracing::error!("Failed to decode base64 content: {}", e);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new("Invalid base64 content")),
                )
                    .into_response());
            }
        };

    // Edit file
    let file = state
        .file_service
        .edit_file(
            file_id,
            auth.user_id,
            content,
            &req.save_mode,
            req.change_description,
        )
        .await
        .map_err(file_error_response)?;

    Ok(Json(EditFileResponse {
        id: file.id,
        current_version: file.current_version,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
        saved_as_new_version: req.save_mode == "new_version",
    }))
}""",
    """pub async fn edit_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<EditFileRequest>,
) -> Result<Json<EditFileResponse>, AppError> {
    // Validate save mode
    if req.save_mode != "overwrite" && req.save_mode != "new_version" {
        return Err(AppError::bad_request(
            "Invalid save_mode. Must be 'overwrite' or 'new_version'",
        ));
    }

    // Decode base64 content
    let content =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.content)
            .map_err(|e| {
                tracing::error!("Failed to decode base64 content: {}", e);
                AppError::bad_request("Invalid base64 content")
            })?;
    let content = Bytes::from(content);

    // Edit file
    let file = state
        .file_service
        .edit_file(
            file_id,
            auth.user_id,
            content,
            &req.save_mode,
            req.change_description,
        )
        .await?;

    Ok(Json(EditFileResponse {
        id: file.id,
        current_version: file.current_version,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
        saved_as_new_version: req.save_mode == "new_version",
    }))
}""",
)

# 17. Replace list_files sqlx map_err
content = content.replace(
    """    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| file_error_response(FileError::Storage(format!("Failed to list files: {}", e))))?;""",
    """    .fetch_all(&state.db_pool)
    .await?;""",
)

# 18. Replace toggle_file_star
content = content.replace(
    """pub async fn toggle_file_star(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<WorkspaceStarRequest>,
) -> Result<StatusCode, AppError> {
    let updated = state
        .metadata_store
        .set_file_starred(file_id, auth.user_id, req.starred)
        .await
        .map_err(|e| {
            file_error_response(FileError::Storage(format!(
                "Failed to update star state: {}",
                e
            )))
        })?;

    if !updated {
        return Err(file_error_response(FileError::NotFound(file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
    """pub async fn toggle_file_star(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<WorkspaceStarRequest>,
) -> Result<StatusCode, AppError> {
    let updated = state
        .metadata_store
        .set_file_starred(file_id, auth.user_id, req.starred)
        .await
        .map_err(|e| AppError::internal(format!(
            "Failed to update star state: {}",
            e
        )))?;

    if !updated {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
)

# 19. Replace restore_file_from_trash
content = content.replace(
    """pub async fn restore_file_from_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let restored = state
        .metadata_store
        .restore_file(file_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| {
            file_error_response(FileError::Storage(format!("Failed to restore file: {}", e)))
        })?;

    if !restored {
        return Err(file_error_response(FileError::NotFound(file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
    """pub async fn restore_file_from_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let restored = state
        .metadata_store
        .restore_file(file_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to restore file: {}", e)))?;

    if !restored {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
)

# 20. Replace permanently_delete_file
content = content.replace(
    """pub async fn permanently_delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .metadata_store
        .permanently_delete_file(file_id, auth.user_id)
        .await
        .map_err(|e| {
            file_error_response(FileError::Storage(format!(
                "Failed to permanently delete file: {}",
                e
            )))
        })?;

    if !deleted {
        return Err(file_error_response(FileError::NotFound(file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
    """pub async fn permanently_delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .metadata_store
        .permanently_delete_file(file_id, auth.user_id)
        .await
        .map_err(|e| AppError::internal(format!(
            "Failed to permanently delete file: {}",
            e
        )))?;

    if !deleted {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}""",
)

# 21. Replace list_starred_items sqlx map_err blocks
content = content.replace(
    """    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to list starred folders: {}",
            e
        )))
    })?;""",
    """    .fetch_all(&state.db_pool)
    .await?;""",
)

content = content.replace(
    """    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to list starred files: {}",
            e
        )))
    })?;""",
    """    .fetch_all(&state.db_pool)
    .await?;""",
)

# 22. Replace list_deleted_items sqlx map_err blocks
content = content.replace(
    """    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to list deleted folders: {}",
            e
        )))
    })?;""",
    """    .fetch_all(&state.db_pool)
    .await?;""",
)

content = content.replace(
    """    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to list deleted files: {}",
            e
        )))
    })?;""",
    """    .fetch_all(&state.db_pool)
    .await?;""",
)

with open(path, "w") as f:
    f.write(content)

print("Done")
