#!/usr/bin/env python3
"""
Script to bulk-add utoipa annotations to RustShare handlers.
"""

import re
from pathlib import Path

BACKEND_ROOT = Path("/srv/data02/projects/rustshare/backend")
ROUTES_FILE = BACKEND_ROOT / "server/src/routes.rs"
HANDLERS_DIR = BACKEND_ROOT / "server/src/handlers"
SERVICES_DIR = BACKEND_ROOT / "server/src/services"

MODULE_TAGS = {
    "files.rs": "Files",
    "folders.rs": "Folders",
    "notes.rs": "Notes",
    "shares.rs": "Shares",
    "users.rs": "Users",
    "groups.rs": "Groups",
    "notifications.rs": "Notifications",
    "decisions.rs": "Decisions",
    "meetings.rs": "Meetings",
    "standups.rs": "Standups",
    "kanban.rs": "Kanban",
    "brainstorming.rs": "Brainstorming",
    "vault_sync.rs": "Vault Sync",
    "public_shares.rs": "Public Shares",
    "replication_handlers.rs": "Replication",
    "chat_integration.rs": "Chat Integration",
    "upload.rs": "Uploads",
    "profile.rs": "Users",
    "devices.rs": "Devices",
    "trash.rs": "Trash",
    "modules.rs": "Modules",
    "invites.rs": "Invites",
    "features.rs": "Features",
    "ai.rs": "AI",
    "auth.rs": "Auth",
    "device_auth.rs": "Auth",
    "workspace_surface.rs": "Modules",
}

SKIP_HANDLERS = {
    "sync_handler", "collab_handler", "validate_client_token",
    "resolve_ws_client_identity", "webdav_root", "webdav_path",
    "provision_user", "deprovision_user", "provision_group", "delete_group",
    "list_users", "get_user", "create_user", "update_user", "patch_user",
    "delete_user", "get_group", "create_group", "update_group", "patch_group",
    "get_service_provider_config", "get_resource_types", "get_schemas",
}


def parse_routes():
    text = ROUTES_FILE.read_text()
    parts = text.split('.route(')
    mapping = {}
    method_pattern = re.compile(r'(get|post|put|patch|delete)\s*\(\s*([^)]+)\s*\)')

    for part in parts[1:]:
        path_match = re.search(r'"([^"]+)"', part)
        if not path_match:
            continue
        path = path_match.group(1)

        block_end = len(part)
        for marker in ['.route(', '.layer(']:
            idx = part.find(marker)
            if idx != -1 and idx < block_end:
                block_end = idx

        block = part[:block_end]
        for m in method_pattern.finditer(block):
            method = m.group(1).upper()
            handler_ref = m.group(2).strip()
            if '::' in handler_ref:
                handler_name = handler_ref.split('::')[-1]
            else:
                handler_name = handler_ref

            if handler_name in SKIP_HANDLERS:
                continue
            mapping[handler_name] = (method, path)

    return mapping


def infer_response_body(func_text):
    arrow_match = re.search(r'->\s*([^\{]+)', func_text)
    if not arrow_match:
        return None
    return_type = arrow_match.group(1).strip()
    json_match = re.search(r'Json<([^>]+)>', return_type)
    if json_match:
        return json_match.group(1).strip()
    return None


def infer_request_body(func_text):
    json_match = re.search(r'Json\s*\(\s*\w+\s*\)\s*:\s*Json<([^>]+)>', func_text)
    if json_match:
        return json_match.group(1).strip()
    vjson_match = re.search(r'ValidatedJson\s*\(\s*\w+\s*\)\s*:\s*ValidatedJson<([^>]+)>', func_text)
    if vjson_match:
        return vjson_match.group(1).strip()
    return None


def has_utoipa_path(text, func_name):
    idx = text.find(f"pub async fn {func_name}")
    if idx == -1:
        return True
    before = text[:idx].rstrip()
    return before.endswith("]") and "utoipa::path" in before[-500:]


def extract_params(func_text):
    params = []
    for match in re.finditer(r'Path\s*\(\s*([^)]+)\s*\)\s*:\s*Path<([^>]+)>', func_text):
        params.append((match.group(1).strip(), match.group(2).strip()))
    tuple_match = re.search(r'Path\s*\(\s*\(([^)]+)\)\s*\)\s*:\s*Path<\s*\(([^)]+)\)\s*>', func_text)
    if tuple_match:
        names = [n.strip() for n in tuple_match.group(1).split(",")]
        types = [t.strip() for t in tuple_match.group(2).split(",")]
        for name, ty in zip(names, types):
            params.append((name, ty))
    return params


def build_annotation(handler_name, method, path, func_text, tag):
    body = infer_response_body(func_text)
    req_body = infer_request_body(func_text)
    params = extract_params(func_text)

    lines = ["#[utoipa::path("]
    lines.append(f"    {method.lower()},")
    lines.append(f'    path = "{path}",')
    lines.append(f'    tag = "{tag}",')

    if params:
        param_strs = [f'("{name}" = {ty}, Path, description = "{name.replace(\"_\", \" \").title()}")' for name, ty in params]
        lines.append(f'    params({", ".join(param_strs)}),')

    if req_body:
        lines.append(f"    request_body = {req_body},")

    responses = []
    if method == "POST" and body and "CREATED" in func_text:
        responses.append(f"(status = 201, description = \"Created\", body = {body})")
    elif method == "DELETE":
        responses.append("(status = 204, description = \"Deleted\")")
    elif method in ("POST", "PUT", "PATCH"):
        if body:
            responses.append(f"(status = 200, description = \"Success\", body = {body})")
        else:
            responses.append("(status = 200, description = \"Success\")")
    elif body:
        responses.append(f"(status = 200, description = \"Success\", body = {body})")
    else:
        responses.append("(status = 200, description = \"Success\")")

    responses.append("(status = 401, description = \"Unauthorized\", body = crate::handlers::ErrorResponse)")
    if params:
        responses.append("(status = 404, description = \"Not found\", body = crate::handlers::ErrorResponse)")

    lines.append("    responses(")
    for r in responses:
        lines.append(f"        {r},")
    lines.append("    ),")
    lines.append(")]")

    return "\n".join(lines)


def process_handler_file(file_path, routes_map):
    text = file_path.read_text()
    original = text
    module_name = file_path.name

    if module_name not in MODULE_TAGS and file_path.parent.name != "admin":
        return False

    tag = MODULE_TAGS.get(module_name, "Admin")

    # Find pub async fn definitions
    # Match the leading whitespace + optional doc comments + pub async fn
    pattern = re.compile(r'(^|\n)(    ///.*\n)*?(    pub async fn (\w+)\s*\()', re.DOTALL)

    changes = []
    for match in pattern.finditer(text):
        func_name = match.group(4)
        func_start = match.start(3)
        func_sig_start = match.start(4)

        if func_name not in routes_map or has_utoipa_path(text[:func_sig_start], func_name):
            continue

        method, path = routes_map[func_name]
        func_end = text.find('{', func_sig_start)
        if func_end == -1:
            continue
        func_text = text[func_sig_start:func_end + 1]

        annotation = build_annotation(func_name, method, path, func_text, tag)
        changes.append((func_start, func_sig_start, annotation))

    for func_start, func_sig_start, annotation in reversed(changes):
        text = text[:func_start] + annotation + "\n    " + text[func_sig_start:]

    if text != original:
        file_path.write_text(text)
        return True
    return False


def add_toschema_to_file(file_path):
    text = file_path.read_text()
    original = text

    # Fast path: check if file has any derives without ToSchema
    if "ToSchema" not in text:
        # Check if it has pub structs with Serialize/Deserialize
        pass

    pattern = re.compile(r'#\[derive\(([^)]*)\)\]')
    matches = list(pattern.finditer(text))

    for match in reversed(matches):
        derives = match.group(1)
        if "ToSchema" in derives:
            continue
        if "Serialize" not in derives and "Deserialize" not in derives:
            continue

        idx = match.end()
        next_part = text[idx:idx + 200].lstrip()
        if not (next_part.startswith("pub struct") or next_part.startswith("pub enum")):
            continue

        new_derive = f'#[derive({derives}, utoipa::ToSchema)]'
        text = text[:match.start()] + new_derive + text[match.end():]

    if text != original:
        file_path.write_text(text)
        return True
    return False


def main():
    print("Parsing routes...")
    routes_map = parse_routes()
    print(f"Found {len(routes_map)} route mappings")

    handler_files = list(HANDLERS_DIR.glob("*.rs")) + list((HANDLERS_DIR / "admin").glob("*.rs"))
    print(f"Found {len(handler_files)} handler files")

    annotated = 0
    for f in handler_files:
        if process_handler_file(f, routes_map):
            annotated += 1
            print(f"  Annotated: {f.name}")

    print(f"\nAnnotated {annotated} handler files")

    toschema = 0
    for f in list(SERVICES_DIR.glob("*.rs")) + handler_files:
        if add_toschema_to_file(f):
            toschema += 1
            print(f"  ToSchema: {f.name}")

    print(f"\nAdded ToSchema to {toschema} files")


if __name__ == "__main__":
    main()
