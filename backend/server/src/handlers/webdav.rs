use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{File, Folder, UserId},
    services::{FileError, FolderError},
};
use uuid::Uuid;

use crate::AppState;

use super::AuthenticatedUser;

const DAV_ROOT: &str = "/dav";

pub async fn webdav_root(
    method: Method,
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    handle_webdav(method, &state, auth.user_id, "/", headers, body).await
}

pub async fn webdav_path(
    Path(path): Path<String>,
    method: Method,
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    let normalized =
        normalize_path(&path).ok_or_else(|| dav_error(StatusCode::BAD_REQUEST, "Invalid path"))?;
    handle_webdav(method, &state, auth.user_id, &normalized, headers, body).await
}

async fn handle_webdav(
    method: Method,
    state: &AppState,
    user_id: UserId,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    match method.as_str() {
        "PROPFIND" => propfind(state, user_id, path, &headers).await,
        "GET" => get_file(state, user_id, path, &headers, false).await,
        "HEAD" => get_file(state, user_id, path, &headers, true).await,
        "PUT" => put_file(state, user_id, path, &headers, body).await,
        "DELETE" => delete_resource(state, user_id, path).await,
        "MOVE" => move_resource(state, user_id, path, &headers).await,
        "MKCOL" => make_collection(state, user_id, path).await,
        _ => Err(dav_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Method not allowed",
        )),
    }
}

async fn propfind(
    state: &AppState,
    user_id: UserId,
    path: &str,
    headers: &HeaderMap,
) -> Result<Response, Response> {
    let depth = headers
        .get("Depth")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("0");

    let resource = resolve_resource(state, user_id, path)
        .await
        .map_err(map_resolve_error)?;

    let mut items = vec![resource.clone()];
    if depth == "1" {
        items.extend(
            list_children(state, user_id, &resource)
                .await
                .map_err(map_storage_error)?,
        );
    }

    let xml = render_multistatus(items);
    // 207 Multi-Status is a valid WebDAV status code
    let status = StatusCode::from_u16(207)
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    Ok((
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml; charset=utf-8"),
        )],
        xml,
    )
        .into_response())
}

async fn get_file(
    state: &AppState,
    user_id: UserId,
    path: &str,
    headers: &HeaderMap,
    head_only: bool,
) -> Result<Response, Response> {
    let resource = resolve_resource(state, user_id, path)
        .await
        .map_err(map_resolve_error)?;
    let file = match resource {
        DavResource::File(file) => file,
        _ => return Err(dav_error(StatusCode::NOT_FOUND, "File not found")),
    };

    let mut content = state
        .object_store
        .get(&file.storage_key())
        .await
        .map_err(map_storage_error)?;

    let mut status = StatusCode::OK;
    let mut content_range: Option<String> = None;

    if let Some(range_header) = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        if let Some((start, end)) = parse_range(range_header, content.len()) {
            let slice = content.slice(start..=end);
            content_range = Some(format!("bytes {}-{}/{}", start, end, content.len()));
            content = slice;
            status = StatusCode::PARTIAL_CONTENT;
        }
    }

    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, file.mime_type.as_str())
        .header(header::ETAG, etag_for_file(&file))
        .header(header::LAST_MODIFIED, format_http_date(file.modified_at))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, content.len().to_string());

    if let Some(content_range) = content_range {
        response = response.header(header::CONTENT_RANGE, content_range);
    }

    response
        .body(if head_only {
            axum::body::Body::empty()
        } else {
            axum::body::Body::from(content)
        })
        .map_err(|_| {
            dav_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build response",
            )
        })
}

async fn put_file(
    state: &AppState,
    user_id: UserId,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    if path == "/" {
        return Err(dav_error(
            StatusCode::CONFLICT,
            "Cannot write the root collection",
        ));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    match resolve_resource(state, user_id, path).await {
        Ok(DavResource::File(existing)) => {
            let expected_version = headers
                .get(header::IF_MATCH)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_if_match_version)
                .ok_or_else(|| {
                    dav_error(
                        StatusCode::PRECONDITION_FAILED,
                        "Missing or invalid If-Match header",
                    )
                })?;

            let updated = state
                .file_service
                .update_file(existing.id, user_id, expected_version, body)
                .await
                .map_err(map_put_error)?;

            let etag = HeaderValue::from_str(&etag_for_file(&updated))
                .unwrap_or_else(|_| HeaderValue::from_static("*"));
            let last_modified = HeaderValue::from_str(&format_http_date(updated.modified_at))
                .unwrap_or_else(|_| HeaderValue::from_static("0"));
            
            response_without_body(
                StatusCode::NO_CONTENT,
                &[
                    (header::ETAG, etag),
                    (header::LAST_MODIFIED, last_modified),
                ],
            )
        }
        Ok(DavResource::Folder(_)) => Err(dav_error(
            StatusCode::CONFLICT,
            "Cannot overwrite a collection with a file",
        )),
        Ok(DavResource::Root) => Err(dav_error(StatusCode::CONFLICT, "Cannot overwrite root")),
        Err(ResolveError::NotFound) => {
            let (parent_id, name) = resolve_parent_for_create(state, user_id, path).await?;
            let created = state
                .file_service
                .upload_file(user_id, name, parent_id, body, content_type)
                .await
                .map_err(map_put_error)?;

            let etag = HeaderValue::from_str(&etag_for_file(&created))
                .unwrap_or_else(|_| HeaderValue::from_static("*"));
            let last_modified = HeaderValue::from_str(&format_http_date(created.modified_at))
                .unwrap_or_else(|_| HeaderValue::from_static("0"));
            
            response_without_body(
                StatusCode::CREATED,
                &[
                    (header::ETAG, etag),
                    (header::LAST_MODIFIED, last_modified),
                ],
            )
        }
        Err(error) => Err(map_resolve_error(error)),
    }
}

async fn delete_resource(
    state: &AppState,
    user_id: UserId,
    path: &str,
) -> Result<Response, Response> {
    match resolve_resource(state, user_id, path).await {
        Ok(DavResource::File(file)) => {
            state
                .file_service
                .delete_file(file.id, user_id)
                .await
                .map_err(map_put_error)?;
            response_without_body(StatusCode::NO_CONTENT, &[])
        }
        Ok(DavResource::Folder(folder)) => {
            state
                .folder_service
                .delete_folder(folder.id, user_id)
                .await
                .map_err(map_folder_error)?;
            response_without_body(StatusCode::NO_CONTENT, &[])
        }
        Ok(DavResource::Root) => Err(dav_error(
            StatusCode::FORBIDDEN,
            "Cannot delete root collection",
        )),
        Err(ResolveError::NotFound) => response_without_body(StatusCode::NOT_FOUND, &[]),
        Err(error) => Err(map_resolve_error(error)),
    }
}

async fn move_resource(
    state: &AppState,
    user_id: UserId,
    source_path: &str,
    headers: &HeaderMap,
) -> Result<Response, Response> {
    let destination = headers
        .get("Destination")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_destination_path)
        .ok_or_else(|| {
            dav_error(
                StatusCode::BAD_REQUEST,
                "Missing or invalid Destination header",
            )
        })?;

    if destination == source_path {
        return response_without_body(StatusCode::NO_CONTENT, &[]);
    }

    let overwrite_allowed = headers
        .get("Overwrite")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("T"))
        .unwrap_or(false);

    if !overwrite_allowed {
        if !matches!(
            resolve_resource(state, user_id, &destination).await,
            Err(ResolveError::NotFound)
        ) {
            return Err(dav_error(
                StatusCode::CONFLICT,
                "Destination already exists",
            ));
        }
    }

    let resource = resolve_resource(state, user_id, source_path)
        .await
        .map_err(map_resolve_error)?;

    match resource {
        DavResource::File(file) => {
            let (target_parent_id, target_name) =
                resolve_parent_for_create(state, user_id, &destination).await?;
            let mut current = file;
            if current.parent_folder_id != target_parent_id {
                current = state
                    .file_service
                    .move_file(current.id, target_parent_id, user_id)
                    .await
                    .map_err(map_put_error)?;
            }
            if current.name != target_name {
                state
                    .file_service
                    .rename_file(current.id, target_name, user_id)
                    .await
                    .map_err(map_put_error)?;
            }
        }
        DavResource::Folder(folder) => {
            let (target_parent_id, target_name) =
                resolve_parent_for_create(state, user_id, &destination).await?;
            let mut current = folder;
            if current.parent_folder_id != target_parent_id {
                current = state
                    .folder_service
                    .move_folder(current.id, target_parent_id, user_id)
                    .await
                    .map_err(map_folder_error)?;
            }
            if current.name != target_name {
                state
                    .folder_service
                    .rename_folder(current.id, target_name, user_id)
                    .await
                    .map_err(map_folder_error)?;
            }
        }
        DavResource::Root => {
            return Err(dav_error(
                StatusCode::FORBIDDEN,
                "Cannot move root collection",
            ))
        }
    }

    response_without_body(StatusCode::NO_CONTENT, &[])
}

async fn make_collection(
    state: &AppState,
    user_id: UserId,
    path: &str,
) -> Result<Response, Response> {
    if path == "/" {
        return Err(dav_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Root collection already exists",
        ));
    }

    if !matches!(
        resolve_resource(state, user_id, path).await,
        Err(ResolveError::NotFound)
    ) {
        return Err(dav_error(
            StatusCode::METHOD_NOT_ALLOWED,
            "Collection already exists",
        ));
    }

    let (parent_id, name) = resolve_parent_for_create(state, user_id, path).await?;
    state
        .folder_service
        .create_folder(name, parent_id, user_id)
        .await
        .map_err(map_folder_error)?;

    response_without_body(StatusCode::CREATED, &[])
}

async fn resolve_parent_for_create(
    state: &AppState,
    user_id: UserId,
    path: &str,
) -> Result<(Option<Uuid>, String), Response> {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| dav_error(StatusCode::BAD_REQUEST, "Invalid destination path"))?
        .to_string();

    let parent_path = parent_path(path);
    if parent_path == "/" {
        return Ok((None, name));
    }

    match resolve_resource(state, user_id, &parent_path).await {
        Ok(DavResource::Folder(folder)) => Ok((Some(folder.id), name)),
        Ok(_) => Err(dav_error(
            StatusCode::CONFLICT,
            "Parent is not a collection",
        )),
        Err(ResolveError::NotFound) => Err(dav_error(
            StatusCode::CONFLICT,
            "Parent collection not found",
        )),
        Err(error) => Err(map_resolve_error(error)),
    }
}

async fn resolve_resource(
    state: &AppState,
    user_id: UserId,
    path: &str,
) -> Result<DavResource, ResolveError> {
    if path == "/" {
        return Ok(DavResource::Root);
    }

    if let Some(folder) = state
        .metadata_store
        .find_folder_by_path(path, user_id)
        .await
        .map_err(ResolveError::Storage)?
    {
        return Ok(DavResource::Folder(folder));
    }

    if let Some(file) = state
        .metadata_store
        .find_file_by_path(path, user_id)
        .await
        .map_err(ResolveError::Storage)?
    {
        return Ok(DavResource::File(file));
    }

    Err(ResolveError::NotFound)
}

async fn list_children(
    state: &AppState,
    user_id: UserId,
    resource: &DavResource,
) -> anyhow::Result<Vec<DavResource>> {
    match resource {
        DavResource::Root => {
            let folders = state.metadata_store.list_folders(None, user_id).await?;
            let files = state.metadata_store.list_files(None, user_id).await?;
            Ok(folders
                .into_iter()
                .map(DavResource::Folder)
                .chain(files.into_iter().map(DavResource::File))
                .collect())
        }
        DavResource::Folder(folder) => {
            let folders = state
                .metadata_store
                .list_folders(Some(folder.id), user_id)
                .await?;
            let files = state
                .metadata_store
                .list_files(Some(folder.id), user_id)
                .await?;
            Ok(folders
                .into_iter()
                .map(DavResource::Folder)
                .chain(files.into_iter().map(DavResource::File))
                .collect())
        }
        DavResource::File(_) => Ok(vec![]),
    }
}

fn render_multistatus(resources: Vec<DavResource>) -> String {
    let body = resources
        .into_iter()
        .map(render_response)
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:">{body}</d:multistatus>"#
    )
}

fn render_response(resource: DavResource) -> String {
    match resource {
        DavResource::Root => format!(
            "<d:response><d:href>{}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
            xml_escape(&dav_href("/"))
        ),
        DavResource::Folder(folder) => format!(
            "<d:response><d:href>{}</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype><d:getetag>{}</d:getetag><d:getlastmodified>{}</d:getlastmodified></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
            xml_escape(&dav_href(&folder.path)),
            xml_escape(&etag_for_folder(&folder)),
            xml_escape(&format_http_date(folder.updated_at))
        ),
        DavResource::File(file) => format!(
            "<d:response><d:href>{}</d:href><d:propstat><d:prop><d:resourcetype/><d:getetag>{}</d:getetag><d:getlastmodified>{}</d:getlastmodified><d:getcontentlength>{}</d:getcontentlength></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
            xml_escape(&dav_href(&file.path)),
            xml_escape(&etag_for_file(&file)),
            xml_escape(&format_http_date(file.modified_at)),
            file.size
        ),
    }
}

fn response_without_body(
    status: StatusCode,
    headers: &[(header::HeaderName, HeaderValue)],
) -> Result<Response, Response> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    builder.body(axum::body::Body::empty()).map_err(|_| {
        dav_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to build response",
        )
    })
}

fn map_put_error(error: FileError) -> Response {
    match error {
        FileError::VersionConflict { .. } => {
            dav_error(StatusCode::PRECONDITION_FAILED, "ETag precondition failed")
        }
        FileError::NotFound(_)
        | FileError::FolderNotFound(_)
        | FileError::ParentFolderNotFound(_) => {
            dav_error(StatusCode::NOT_FOUND, &error.to_string())
        }
        FileError::PermissionDenied { .. } => dav_error(StatusCode::FORBIDDEN, &error.to_string()),
        FileError::InvalidName(_) => dav_error(StatusCode::BAD_REQUEST, &error.to_string()),
        FileError::QuotaExceeded { .. } => dav_error(StatusCode::FORBIDDEN, &error.to_string()),
        FileError::VersionNotFound(_) => dav_error(StatusCode::NOT_FOUND, &error.to_string()),
        FileError::Database(_) | FileError::Storage(_) => {
            dav_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        }
    }
}

fn map_folder_error(error: FolderError) -> Response {
    match error {
        FolderError::NotFound(_) | FolderError::ParentFolderNotFound(_) => {
            dav_error(StatusCode::NOT_FOUND, &error.to_string())
        }
        FolderError::PermissionDenied { .. } => {
            dav_error(StatusCode::FORBIDDEN, &error.to_string())
        }
        FolderError::CircularReference { .. } => {
            dav_error(StatusCode::CONFLICT, &error.to_string())
        }
        FolderError::DuplicateName { .. } => dav_error(StatusCode::CONFLICT, &error.to_string()),
        FolderError::InvalidName(_) | FolderError::CannotDeleteRoot(_) => {
            dav_error(StatusCode::BAD_REQUEST, &error.to_string())
        }
        FolderError::Database(_) => {
            dav_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        }
    }
}

fn map_storage_error(error: anyhow::Error) -> Response {
    dav_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
}

fn map_resolve_error(error: ResolveError) -> Response {
    match error {
        ResolveError::NotFound => dav_error(StatusCode::NOT_FOUND, "Resource not found"),
        ResolveError::Storage(error) => map_storage_error(error),
    }
}

fn dav_error(status: StatusCode, message: &str) -> Response {
    (status, message.to_string()).into_response()
}

fn normalize_path(raw_path: &str) -> Option<String> {
    let mut segments = Vec::new();
    for segment in raw_path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." {
            return None;
        }
        segments.push(segment);
    }

    Some(if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    })
}

fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((prefix, _)) if !prefix.is_empty() => prefix.to_string(),
        _ => "/".to_string(),
    }
}

fn parse_if_match_version(raw: &str) -> Option<i32> {
    raw.trim()
        .trim_matches('"')
        .trim_start_matches('v')
        .parse()
        .ok()
}

fn parse_destination_path(raw: &str) -> Option<String> {
    let path = if raw.starts_with("http://") || raw.starts_with("https://") {
        let url = url::Url::parse(raw).ok()?;
        url.path().to_string()
    } else {
        raw.to_string()
    };

    let relative = path.strip_prefix(DAV_ROOT).unwrap_or(&path);
    normalize_path(relative)
}

fn parse_range(raw: &str, total_len: usize) -> Option<(usize, usize)> {
    let bytes = raw.strip_prefix("bytes=")?;
    let (start, end) = bytes.split_once('-')?;
    let start = start.parse::<usize>().ok()?;
    let end = if end.is_empty() {
        total_len.checked_sub(1)?
    } else {
        end.parse::<usize>().ok()?
    };

    if start > end || end >= total_len {
        return None;
    }

    Some((start, end))
}

fn etag_for_file(file: &File) -> String {
    format!("\"v{}\"", file.current_version)
}

fn etag_for_folder(folder: &Folder) -> String {
    format!("\"folder-{}-{}\"", folder.id, folder.updated_at.timestamp())
}

fn format_http_date(date: DateTime<Utc>) -> String {
    date.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn dav_href(path: &str) -> String {
    if path == "/" {
        format!("{}/", DAV_ROOT)
    } else {
        format!("{}{}", DAV_ROOT, path)
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Clone)]
enum DavResource {
    Root,
    Folder(Folder),
    File(File),
}

enum ResolveError {
    NotFound,
    Storage(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_rejects_parent_traversal() {
        assert_eq!(
            normalize_path("Documents/report.txt"),
            Some("/Documents/report.txt".to_string())
        );
        assert_eq!(normalize_path("../etc/passwd"), None);
    }

    #[test]
    fn parse_if_match_accepts_webdav_etags() {
        assert_eq!(parse_if_match_version("\"v12\""), Some(12));
        assert_eq!(parse_if_match_version("7"), Some(7));
        assert_eq!(parse_if_match_version("\"bogus\""), None);
    }

    #[test]
    fn parse_destination_strips_dav_prefix() {
        assert_eq!(
            parse_destination_path("http://localhost:8080/dav/Documents/report.txt"),
            Some("/Documents/report.txt".to_string())
        );
        assert_eq!(
            parse_destination_path("/dav/Documents"),
            Some("/Documents".to_string())
        );
    }

    #[test]
    fn parse_range_supports_closed_and_open_ended_ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=10-", 20), Some((10, 19)));
        assert_eq!(parse_range("items=1-2", 20), None);
    }
}
