use crate::state::AppState;
use axum::Router;

pub fn health_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new()
        .route("/health", get(crate::health_check))
        .route("/health/ready", get(crate::handlers::readiness_check))
}

pub fn auth_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route("/api/v1/auth/config", get(crate::oidc::auth_config))
        .route("/api/v1/auth/login", post(crate::handlers::login))
        .route("/api/v1/auth/logout", post(crate::handlers::logout))
        .route("/api/v1/auth/oidc/login", get(crate::oidc::oidc_login))
        .route(
            "/api/v1/auth/oidc/callback",
            get(crate::oidc::oidc_callback),
        )
        .route(
            "/api/v1/auth/oidc/mobile/authorize",
            post(crate::oidc::mobile_oidc_authorize),
        )
        .route(
            "/api/v1/auth/oidc/mobile/exchange",
            post(crate::oidc::mobile_oidc_exchange),
        )
}

pub fn device_auth_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route(
            "/api/v1/auth/device/qr-info",
            get(crate::handlers::device_auth::device_qr_info),
        )
        .route(
            "/api/v1/auth/device/request",
            post(crate::handlers::device_auth::device_request),
        )
        .route(
            "/api/v1/auth/device/poll",
            post(crate::handlers::device_auth::device_poll),
        )
        .route(
            "/api/v1/auth/device/approve",
            post(crate::handlers::device_auth::device_approve),
        )
}

pub fn device_management_routes() -> Router<AppState> {
    use axum::routing::{delete, get};
    Router::new()
        .route(
            "/api/v1/user/devices",
            get(crate::handlers::devices::list_devices),
        )
        .route(
            "/api/v1/user/devices/{id}",
            delete(crate::handlers::devices::revoke_device),
        )
}

pub fn feature_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new().route("/api/v1/features", get(crate::handlers::get_features))
}

pub fn file_routes() -> Router<AppState> {
    use axum::extract::DefaultBodyLimit;
    use axum::routing::{delete, get, patch, post, put};
    Router::new()
        .route("/api/v1/files", get(crate::handlers::list_files))
        .route(
            "/api/v1/files/starred",
            get(crate::handlers::list_starred_items),
        )
        .route(
            "/api/v1/files/deleted",
            get(crate::handlers::list_deleted_items),
        )
        .route(
            "/api/v1/files/upload",
            post(crate::handlers::upload_file).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/v1/files/{id}", get(crate::handlers::get_file))
        .route("/api/v1/files/{id}", put(crate::handlers::update_file))
        .route("/api/v1/files/{id}", delete(crate::handlers::delete_file))
        .route(
            "/api/v1/files/{id}/star",
            patch(crate::handlers::toggle_file_star),
        )
        .route(
            "/api/v1/files/{id}/restore-from-trash",
            post(crate::handlers::restore_file_from_trash),
        )
        .route(
            "/api/v1/files/{id}/permanent",
            delete(crate::handlers::permanently_delete_file),
        )
        .route(
            "/api/v1/files/{id}/download",
            get(crate::handlers::download_file),
        )
        .route(
            "/api/v1/files/{id}/content",
            get(crate::handlers::download_file_content),
        )
        .route(
            "/api/v1/files/{id}/preview",
            get(crate::handlers::preview_file),
        )
        .route(
            "/api/v1/files/{id}/versions",
            get(crate::handlers::get_file_versions),
        )
        .route(
            "/api/v1/files/{id}/restore",
            post(crate::handlers::restore_file_version),
        )
        .route("/api/v1/files/{id}/move", post(crate::handlers::move_file))
        .route(
            "/api/v1/files/{id}/rename",
            post(crate::handlers::rename_file),
        )
        .route(
            "/api/v1/files/{id}/thumbnail",
            get(crate::handlers::get_file_thumbnail),
        )
        .route("/api/v1/files/{id}/edit", post(crate::handlers::edit_file))
}

pub fn upload_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route(
            "/api/v1/uploads/sessions",
            post(crate::handlers::upload::create_upload_session),
        )
        .route(
            "/api/v1/uploads/sessions",
            get(crate::handlers::upload::list_upload_sessions),
        )
        .route(
            "/api/v1/uploads/sessions/{id}",
            get(crate::handlers::upload::get_upload_session_status),
        )
        .route(
            "/api/v1/uploads/sessions/{id}",
            delete(crate::handlers::upload::abort_upload_session),
        )
        .route(
            "/api/v1/uploads/sessions/{id}/chunks/{index}",
            put(crate::handlers::upload::upload_chunk),
        )
        .route(
            "/api/v1/uploads/sessions/{id}/complete",
            post(crate::handlers::upload::complete_upload),
        )
}

pub fn note_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route("/api/v1/notes", post(crate::handlers::create_note))
        .route("/api/v1/notes", get(crate::handlers::list_notes))
        .route(
            "/api/v1/notes/recent",
            get(crate::handlers::list_recent_notes),
        )
        .route("/api/v1/notes/{id}", get(crate::handlers::get_note))
        .route("/api/v1/notes/{id}", put(crate::handlers::save_note))
        .route(
            "/api/v1/notes/{id}/rename",
            post(crate::handlers::rename_note),
        )
        .route("/api/v1/notes/{id}/move", post(crate::handlers::move_note))
        .route(
            "/api/v1/notes/{id}/visibility",
            post(crate::handlers::toggle_visibility),
        )
        .route("/api/v1/notes/{id}", delete(crate::handlers::delete_note))
        .route(
            "/api/v1/notes/{id}/duplicate",
            post(crate::handlers::duplicate_note),
        )
}

pub fn note_public_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new().route(
        "/api/v1/public/notes/{share_id}",
        get(crate::handlers::get_public_note),
    )
}

pub fn replication_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new()
        .route(
            "/api/admin/replication/jobs",
            get(crate::replication_handlers::list_replication_jobs),
        )
        .route(
            "/api/admin/replication/summary",
            get(crate::replication_handlers::get_replication_summary),
        )
        .route(
            "/api/admin/replication/targets",
            get(crate::replication_handlers::list_replication_targets),
        )
        .route(
            "/api/v1/files/{id}/replication",
            get(crate::replication_handlers::get_file_replication_status),
        )
        .route(
            "/api/v1/admin/replication/jobs",
            get(crate::replication_handlers::list_replication_jobs),
        )
        .route(
            "/api/v1/admin/replication/summary",
            get(crate::replication_handlers::get_replication_summary),
        )
        .route(
            "/api/v1/admin/replication/targets",
            get(crate::replication_handlers::list_replication_targets),
        )
}

pub fn module_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route(
            "/api/v1/modules",
            get(crate::handlers::list_enabled_modules),
        )
        .route("/api/v1/modules/{key}", get(crate::handlers::get_module))
        .route(
            "/api/v1/modules/{key}/summary",
            get(crate::handlers::get_module_summary),
        )
        .route(
            "/api/v1/workspace-surface",
            get(crate::handlers::get_workspace_surface),
        )
        .route(
            "/api/v1/modules/from-template",
            post(crate::handlers::create_from_template),
        )
}

pub fn kanban_routes() -> Router<AppState> {
    use axum::routing::{delete, get, patch, post, put};
    Router::new()
        .route(
            "/api/v1/modules/kanban/boards",
            get(crate::handlers::list_boards),
        )
        .route(
            "/api/v1/modules/kanban/boards",
            post(crate::handlers::create_board),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}",
            get(crate::handlers::get_board),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}",
            patch(crate::handlers::update_board),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/archive",
            post(crate::handlers::archive_board),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/cards",
            get(crate::handlers::list_cards),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/cards",
            post(crate::handlers::create_card),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}",
            get(crate::handlers::get_card),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/detail",
            get(crate::handlers::get_card_detail),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}",
            patch(crate::handlers::update_card),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/description",
            put(crate::handlers::update_card_description),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/move",
            post(crate::handlers::move_card),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/archive",
            post(crate::handlers::archive_card),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}",
            delete(crate::handlers::delete_card),
        )
        // Labels
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/labels",
            post(crate::handlers::create_label),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/labels/{label_id}",
            patch(crate::handlers::update_label),
        )
        .route(
            "/api/v1/modules/kanban/boards/{board_id}/labels/{label_id}",
            delete(crate::handlers::delete_label),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/labels",
            post(crate::handlers::add_card_label),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/labels/{label_id}",
            delete(crate::handlers::remove_card_label),
        )
        // Assignees
        .route(
            "/api/v1/modules/kanban/assignable-users",
            get(crate::handlers::get_assignable_users),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/assignees",
            post(crate::handlers::assign_card_member),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/assignees/{assignee_id}",
            delete(crate::handlers::unassign_card_member),
        )
        // Attachments
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/attachments",
            post(crate::handlers::add_card_attachment),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/attachments/{attachment_id}",
            delete(crate::handlers::delete_card_attachment),
        )
        // Checklists
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/checklists",
            post(crate::handlers::create_checklist),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/checklists/{checklist_id}/items",
            post(crate::handlers::create_checklist_item),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/checklists/{checklist_id}/items/{item_id}",
            patch(crate::handlers::toggle_checklist_item),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/checklists/{checklist_id}/items/{item_id}",
            delete(crate::handlers::delete_checklist_item),
        )
        .route(
            "/api/v1/modules/kanban/cards/{card_id}/checklists/{checklist_id}",
            delete(crate::handlers::delete_checklist),
        )
}

pub fn decision_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route("/api/v1/decisions", get(crate::handlers::list_decisions))
        .route("/api/v1/decisions", post(crate::handlers::create_decision))
        .route("/api/v1/decisions/{id}", get(crate::handlers::get_decision))
        .route(
            "/api/v1/decisions/{id}",
            put(crate::handlers::update_decision),
        )
        .route(
            "/api/v1/decisions/{id}",
            delete(crate::handlers::delete_decision),
        )
        .route(
            "/api/v1/decisions/{id}/rename",
            post(crate::handlers::rename_decision),
        )
}

pub fn meeting_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route("/api/v1/meetings", get(crate::handlers::list_meetings))
        .route("/api/v1/meetings", post(crate::handlers::create_meeting))
        .route("/api/v1/meetings/{id}", get(crate::handlers::get_meeting))
        .route(
            "/api/v1/meetings/{id}",
            put(crate::handlers::update_meeting),
        )
        .route(
            "/api/v1/meetings/{id}",
            delete(crate::handlers::delete_meeting),
        )
}

pub fn standup_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route("/api/v1/standups", get(crate::handlers::list_standups))
        .route("/api/v1/standups", post(crate::handlers::create_standup))
        .route("/api/v1/standups/{id}", get(crate::handlers::get_standup))
        .route(
            "/api/v1/standups/{id}",
            put(crate::handlers::update_standup),
        )
        .route(
            "/api/v1/standups/{id}",
            delete(crate::handlers::delete_standup),
        )
}

pub fn brainstorming_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route(
            "/api/v1/modules/brainstorming/boards",
            get(crate::handlers::list_brainstorm_boards),
        )
        .route(
            "/api/v1/modules/brainstorming/boards",
            post(crate::handlers::create_brainstorm_board),
        )
        .route(
            "/api/v1/modules/brainstorming/boards/{board_id}",
            get(crate::handlers::get_brainstorm_board),
        )
        .route(
            "/api/v1/modules/brainstorming/boards/{board_id}/source",
            get(crate::handlers::get_brainstorm_board_source),
        )
        .route(
            "/api/v1/modules/brainstorming/boards/{board_id}/source",
            put(crate::handlers::save_brainstorm_board_source),
        )
        .route(
            "/api/v1/modules/brainstorming/boards/{board_id}/preview",
            put(crate::handlers::update_brainstorm_board_preview),
        )
        .route(
            "/api/v1/modules/brainstorming/boards/{board_id}",
            delete(crate::handlers::delete_brainstorm_board),
        )
}

pub fn admin_routes() -> Router<AppState> {
    use axum::routing::{delete, get, patch, post, put};
    Router::new()
        .route(
            "/api/v1/admin/modules",
            get(crate::handlers::admin::modules::list_modules),
        )
        .route(
            "/api/v1/admin/modules/{key}",
            get(crate::handlers::admin::modules::get_module),
        )
        .route(
            "/api/v1/admin/modules/{key}/enable",
            post(crate::handlers::admin::modules::enable_module),
        )
        .route(
            "/api/v1/admin/modules/{key}/disable",
            post(crate::handlers::admin::modules::disable_module),
        )
        .route(
            "/api/v1/admin/modules/{key}",
            patch(crate::handlers::admin::modules::update_module),
        )
        .route(
            "/api/v1/admin/modules/{key}/templates",
            get(crate::handlers::admin::templates::list_templates_by_module),
        )
        .route(
            "/api/v1/admin/templates",
            get(crate::handlers::admin::templates::list_templates),
        )
        .route(
            "/api/v1/admin/templates",
            post(crate::handlers::admin::templates::create_template),
        )
        .route(
            "/api/v1/admin/templates/{key}",
            get(crate::handlers::admin::templates::get_template),
        )
        .route(
            "/api/v1/admin/templates/{key}",
            put(crate::handlers::admin::templates::update_template),
        )
        .route(
            "/api/v1/admin/templates/{key}",
            delete(crate::handlers::admin::templates::delete_template),
        )
        .route(
            "/api/v1/admin/templates/{key}/duplicate",
            post(crate::handlers::admin::templates::duplicate_template),
        )
        .route(
            "/api/v1/admin/users",
            get(crate::handlers::admin::users::list_admin_users),
        )
        .route(
            "/api/v1/admin/users",
            post(crate::handlers::admin::users::create_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            get(crate::handlers::admin::users::get_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            patch(crate::handlers::admin::users::update_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}/disable",
            post(crate::handlers::admin::users::disable_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}/enable",
            post(crate::handlers::admin::users::enable_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            delete(crate::handlers::admin::users::delete_admin_user),
        )
        .route(
            "/api/v1/admin/audit",
            get(crate::handlers::admin::audit::list_audit_log),
        )
        .route(
            "/api/v1/admin/workflows",
            get(crate::handlers::admin::workflows::list_workflows),
        )
        .route(
            "/api/v1/admin/workflows/{id}",
            get(crate::handlers::admin::workflows::get_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}",
            put(crate::handlers::admin::workflows::update_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}/enable",
            post(crate::handlers::admin::workflows::enable_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}/disable",
            post(crate::handlers::admin::workflows::disable_workflow),
        )
        .route(
            "/api/v1/admin/groups",
            get(crate::handlers::admin::groups::list_groups),
        )
        .route(
            "/api/v1/admin/groups",
            post(crate::handlers::admin::groups::create_group),
        )
        .route(
            "/api/v1/admin/groups/{id}",
            get(crate::handlers::admin::groups::get_group),
        )
        .route(
            "/api/v1/admin/groups/{id}",
            patch(crate::handlers::admin::groups::update_group),
        )
        .route(
            "/api/v1/admin/groups/{id}",
            delete(crate::handlers::admin::groups::delete_group),
        )
        .route(
            "/api/v1/admin/groups/{id}/members",
            post(crate::handlers::admin::groups::add_member),
        )
        .route(
            "/api/v1/admin/groups/{id}/members/{user_id}",
            delete(crate::handlers::admin::groups::remove_member),
        )
        .route(
            "/api/v1/admin/config/oidc",
            get(crate::handlers::admin::config::get_oidc_config),
        )
        .route(
            "/api/v1/admin/config/oidc",
            put(crate::handlers::admin::config::update_oidc_config),
        )
        .route(
            "/api/v1/admin/config/oidc/test",
            post(crate::handlers::admin::config::test_oidc_config),
        )
        .route(
            "/api/v1/admin/config/smtp",
            get(crate::handlers::admin::config::get_smtp_config),
        )
        .route(
            "/api/v1/admin/config/smtp",
            put(crate::handlers::admin::config::update_smtp_config),
        )
        .route(
            "/api/v1/admin/config/smtp/test",
            post(crate::handlers::admin::config::test_smtp_config),
        )
        .route(
            "/api/v1/admin/config/security",
            get(crate::handlers::admin::config::get_security_config),
        )
        .route(
            "/api/v1/admin/config/security",
            put(crate::handlers::admin::config::update_security_config),
        )
        .route(
            "/api/v1/admin/integrations/webhooks",
            get(crate::handlers::admin::webhooks::list_webhooks),
        )
        .route(
            "/api/v1/admin/integrations/webhooks",
            post(crate::handlers::admin::webhooks::create_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/{id}",
            patch(crate::handlers::admin::webhooks::update_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/{id}",
            delete(crate::handlers::admin::webhooks::delete_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/{id}/test",
            post(crate::handlers::admin::webhooks::test_webhook),
        )
}

pub fn scim_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post};
    Router::new()
        .route(
            "/api/v1/scim/users",
            post(crate::handlers::scim::provision_user),
        )
        .route(
            "/api/v1/scim/users/{external_id}",
            delete(crate::handlers::scim::deprovision_user),
        )
        .route(
            "/api/v1/scim/groups",
            post(crate::handlers::scim::provision_group),
        )
        .route(
            "/api/v1/scim/groups/{external_id}",
            delete(crate::handlers::scim::delete_group),
        )
        .route(
            "/scim/v2/Users",
            get(crate::handlers::scim_v2::list_users).post(crate::handlers::scim_v2::create_user),
        )
        .route(
            "/scim/v2/Users/{id}",
            get(crate::handlers::scim_v2::get_user)
                .put(crate::handlers::scim_v2::update_user)
                .patch(crate::handlers::scim_v2::patch_user)
                .delete(crate::handlers::scim_v2::delete_user),
        )
        .route(
            "/scim/v2/Groups",
            get(crate::handlers::scim_v2::list_groups).post(crate::handlers::scim_v2::create_group),
        )
        .route(
            "/scim/v2/Groups/{id}",
            get(crate::handlers::scim_v2::get_group)
                .put(crate::handlers::scim_v2::update_group)
                .patch(crate::handlers::scim_v2::patch_group)
                .delete(crate::handlers::scim_v2::delete_group),
        )
        .route(
            "/scim/v2/ServiceProviderConfig",
            get(crate::handlers::scim_v2::get_service_provider_config),
        )
        .route(
            "/scim/v2/ResourceTypes",
            get(crate::handlers::scim_v2::get_resource_types),
        )
        .route(
            "/scim/v2/Schemas",
            get(crate::handlers::scim_v2::get_schemas),
        )
}

pub fn folder_routes() -> Router<AppState> {
    use axum::routing::{delete, get, patch, post};
    Router::new()
        .route("/api/v1/folders", post(crate::handlers::create_folder))
        .route(
            "/api/v1/folders/root/contents",
            get(crate::handlers::get_root_contents),
        )
        .route(
            "/api/v1/folders/tree",
            get(crate::handlers::get_folder_tree),
        )
        .route(
            "/api/v1/folders/{id}/contents",
            get(crate::handlers::get_folder_contents),
        )
        .route(
            "/api/v1/folders/{id}/star",
            patch(crate::handlers::toggle_folder_star),
        )
        .route(
            "/api/v1/folders/{id}/restore-from-trash",
            post(crate::handlers::restore_folder_from_trash),
        )
        .route(
            "/api/v1/folders/{id}/permanent",
            delete(crate::handlers::permanently_delete_folder),
        )
        .route(
            "/api/v1/folders/{id}/move",
            post(crate::handlers::move_folder),
        )
        .route(
            "/api/v1/folders/{id}/rename",
            post(crate::handlers::rename_folder),
        )
        .route("/api/v1/folders/{id}", get(crate::handlers::get_folder))
        .route(
            "/api/v1/folders/{id}",
            delete(crate::handlers::delete_folder),
        )
}

pub fn share_routes() -> Router<AppState> {
    use axum::routing::{delete, get, post, put};
    Router::new()
        // Public file/folder shares
        .route(
            "/api/v1/files/{file_id}/shares",
            post(crate::handlers::create_public_file_share),
        )
        .route(
            "/api/v1/folders/{folder_id}/shares",
            post(crate::handlers::create_public_folder_share),
        )
        .route(
            "/api/v1/files/{file_id}/shares",
            get(crate::handlers::list_public_file_shares),
        )
        .route(
            "/api/v1/folders/{folder_id}/shares",
            get(crate::handlers::list_public_folder_shares),
        )
        .route("/api/v1/shares", get(crate::handlers::list_user_shares))
        .route(
            "/api/v1/shares/{id}/access-log",
            get(crate::handlers::get_share_access_log),
        )
        .route("/api/v1/shares/{id}", delete(crate::handlers::revoke_share))
        // Internal user shares
        .route(
            "/api/v1/files/{id}/share",
            post(crate::handlers::create_file_share),
        )
        .route(
            "/api/v1/folders/{id}/share",
            post(crate::handlers::create_folder_share),
        )
        .route(
            "/api/v1/shares/received",
            get(crate::handlers::list_received_shares),
        )
        .route(
            "/api/v1/files/{id}/recipients",
            get(crate::handlers::list_file_recipients),
        )
        .route(
            "/api/v1/folders/{id}/recipients",
            get(crate::handlers::list_folder_recipients),
        )
        .route(
            "/api/v1/shares/{id}/permission",
            put(crate::handlers::update_recipient_permission),
        )
        .route(
            "/api/v1/shares/{id}/recipient",
            delete(crate::handlers::remove_recipient),
        )
        .route(
            "/api/v1/shares/folders/{id}/contents",
            get(crate::handlers::get_user_shared_folder_contents),
        )
        .route(
            "/api/v1/shares/folders/{id}/tree",
            get(crate::handlers::get_user_shared_folder_tree),
        )
        // Group shares
        .route(
            "/api/v1/files/{id}/share/group",
            post(crate::handlers::create_file_group_share),
        )
        .route(
            "/api/v1/files/{id}/share/groups",
            get(crate::handlers::list_file_group_shares),
        )
        .route(
            "/api/v1/folders/{id}/share/group",
            post(crate::handlers::create_folder_group_share),
        )
        .route(
            "/api/v1/folders/{id}/share/groups",
            get(crate::handlers::list_folder_group_shares),
        )
        .route(
            "/api/v1/shares/{id}/group",
            delete(crate::handlers::revoke_group_share),
        )
        .route(
            "/api/v1/shares/{id}/group/permission",
            put(crate::handlers::update_group_share_permission),
        )
}

pub fn user_routes() -> Router<AppState> {
    use axum::routing::{delete, get, patch, post};
    Router::new()
        .route("/api/users/me", get(crate::handlers::get_user_profile))
        .route("/api/v1/users/me", get(crate::handlers::get_user_profile))
        .route("/api/v1/me", get(crate::handlers::get_user_profile))
        .route(
            "/api/users/me/theme",
            patch(crate::handlers::update_user_theme),
        )
        .route(
            "/api/v1/users/me/theme",
            patch(crate::handlers::update_user_theme),
        )
        .route(
            "/api/v1/me/theme",
            patch(crate::handlers::update_user_theme),
        )
        .route(
            "/api/users/me/sessions",
            get(crate::handlers::list_user_sessions),
        )
        .route(
            "/api/v1/users/me/sessions",
            get(crate::handlers::list_user_sessions),
        )
        .route(
            "/api/v1/me/sessions",
            get(crate::handlers::list_user_sessions),
        )
        .route(
            "/api/users/me/security-events",
            get(crate::handlers::list_user_security_events),
        )
        .route(
            "/api/v1/users/me/security-events",
            get(crate::handlers::list_user_security_events),
        )
        .route(
            "/api/v1/me/security-events",
            get(crate::handlers::list_user_security_events),
        )
        .route(
            "/api/users/me/sessions/{id}",
            delete(crate::handlers::delete_user_session),
        )
        .route(
            "/api/v1/users/me/sessions/{id}",
            delete(crate::handlers::delete_user_session),
        )
        .route(
            "/api/v1/me/sessions/{id}",
            delete(crate::handlers::delete_user_session),
        )
        .route(
            "/api/users/me/password",
            patch(crate::handlers::update_user_password),
        )
        .route(
            "/api/v1/users/me/password",
            patch(crate::handlers::update_user_password),
        )
        .route(
            "/api/v1/me/password",
            patch(crate::handlers::update_user_password),
        )
        .route(
            "/api/v1/users/me/profile",
            get(crate::handlers::get_profile),
        )
        .route(
            "/api/v1/users/me/profile",
            patch(crate::handlers::update_profile),
        )
        .route(
            "/api/v1/users/me/trash-retention",
            patch(crate::handlers::update_trash_retention),
        )
        .route(
            "/api/v1/users/me/avatar",
            post(crate::handlers::upload_avatar).delete(crate::handlers::delete_avatar),
        )
        .route(
            "/api/v1/users/{id}/avatar",
            get(crate::handlers::get_avatar),
        )
        .route(
            "/api/v1/users/me/modules",
            get(crate::handlers::list_user_module_preferences),
        )
        .route(
            "/api/v1/users/me/modules/{key}",
            patch(crate::handlers::update_user_module_preference),
        )
        .route(
            "/api/v1/users/me/dashboard-config",
            get(crate::handlers::get_dashboard_config)
                .put(crate::handlers::update_dashboard_config),
        )
}

pub fn group_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new()
        .route("/api/v1/groups/my", get(crate::handlers::list_my_groups))
        .route("/api/v1/groups/my/{id}", get(crate::handlers::get_my_group))
}

pub fn notification_routes() -> Router<AppState> {
    use axum::routing::{delete, get, put};
    Router::new()
        .route(
            "/api/v1/notifications",
            get(crate::handlers::list_notifications),
        )
        .route(
            "/api/v1/notifications/unread-count",
            get(crate::handlers::count_unread_notifications),
        )
        .route(
            "/api/v1/notifications/{id}/read",
            put(crate::handlers::mark_notification_read),
        )
        .route(
            "/api/v1/notifications/{id}",
            delete(crate::handlers::delete_notification),
        )
        .route("/api/v1/activity", get(crate::handlers::list_activity))
}

pub fn invite_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route("/api/v1/invites", post(crate::handlers::create_invite))
        .route("/api/v1/invites/{token}", get(crate::handlers::get_invite))
        .route(
            "/api/v1/invites/{token}/accept",
            post(crate::handlers::accept_invite),
        )
}

pub fn ai_routes() -> Router<AppState> {
    use axum::routing::post;
    Router::new()
        .route("/api/v1/ai/search", post(crate::handlers::semantic_search))
        .route(
            "/api/v1/ai/summarize",
            post(crate::handlers::summarize_file),
        )
        .route("/api/v1/ai/ask", post(crate::handlers::ask_question))
}

pub fn trash_routes() -> Router<AppState> {
    use axum::routing::{delete, get};
    Router::new()
        .route(
            "/api/v1/trash/summary",
            get(crate::handlers::get_trash_summary),
        )
        .route("/api/v1/trash/empty", delete(crate::handlers::empty_trash))
}

pub fn public_share_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route(
            "/api/v1/public/share/{token}/session",
            post(crate::handlers::create_session),
        )
        .route(
            "/api/v1/public/share/{token}/info",
            get(crate::handlers::get_share_info),
        )
        .route(
            "/api/v1/public/share/{token}/file",
            get(crate::handlers::download_shared_file),
        )
        .route(
            "/api/v1/public/share/{token}/folder/contents",
            get(crate::handlers::get_shared_folder_contents),
        )
        .route(
            "/api/v1/public/share/{token}/folder/files/{file_id}",
            get(crate::handlers::download_shared_folder_file),
        )
        .route(
            "/api/v1/public/share/{token}/folder/upload",
            post(crate::handlers::upload_shared_folder_file),
        )
}

/// Vault sync routes.
///
/// Rate limiting is applied globally in `main.rs` via `rate_limit_middleware`,
/// which classifies these endpoints as `VaultSyncRead`, `VaultSyncWrite`, or
/// `VaultSyncUpload` based on method and path.
pub fn vault_sync_routes() -> Router<AppState> {
    use axum::extract::DefaultBodyLimit;
    use axum::routing::{delete, get, post, put};
    Router::new()
        .route(
            "/api/vault-sync/v1/vaults",
            post(crate::handlers::vault_sync::create_vault),
        )
        .route(
            "/api/vault-sync/v1/vaults",
            get(crate::handlers::vault_sync::list_vaults),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}",
            get(crate::handlers::vault_sync::get_vault),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}/manifest",
            get(crate::handlers::vault_sync::get_manifest),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
            get(crate::handlers::vault_sync::download_file),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
            put(crate::handlers::vault_sync::upload_file),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
            delete(crate::handlers::vault_sync::delete_file),
        )
        .route(
            "/api/vault-sync/v1/vaults/{vault_id}/rename",
            post(crate::handlers::vault_sync::rename_file),
        )
        .route(
            "/api/vault-sync/v1/devices/register",
            post(crate::handlers::vault_sync::register_device),
        )
        .route(
            "/api/vault-sync/v1/devices/{device_id}",
            delete(crate::handlers::vault_sync::revoke_device),
        )
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
}

pub fn sync_routes() -> Router<AppState> {
    use axum::routing::get;
    Router::new()
        .route("/api/ws", get(crate::handlers::sync_handler))
        .route("/api/ws/collab", get(crate::handlers::collab_handler))
        .route("/api/v1/sync/cursor", get(crate::handlers::get_sync_cursor))
        .route("/api/v1/sync/delta", get(crate::handlers::get_sync_delta))
}
