pub const APPROVED_ICON_KEYS: &[&str] = &[
    "layout-dashboard",
    "folder",
    "file-text",
    "sticky-note",
    "calendar-days",
    "clipboard-list",
    "columns",
    "git-branch",
    "share-2",
    "lock",
    "globe",
    "settings",
];

pub fn is_approved_icon_key(icon: &str) -> bool {
    APPROVED_ICON_KEYS.contains(&icon)
}
