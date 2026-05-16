use std::os::raw::c_void;
use tracing::info;

#[no_mangle]
pub extern "C" fn isUnicode() -> bool {
    true
}

#[no_mangle]
pub extern "C" fn getName() -> *const u16 {
    static NAME: &[u16] = &[
        'R' as u16,
        'u' as u16,
        's' as u16,
        't' as u16,
        'S' as u16,
        'h' as u16,
        'a' as u16,
        'r' as u16,
        'e' as u16,
        '\0' as u16,
    ];
    NAME.as_ptr()
}

#[no_mangle]
pub extern "C" fn setInfo(
    _h_inst: *mut c_void,
    _npp_handle: *mut c_void,
    _scintilla_handle: *mut c_void,
) {
    // Store handles for later use if needed
}

// Notepad++ notification codes
const NPPN_FIRST: u32 = 1000;
const NPPN_FILESAVED: u32 = NPPN_FIRST + 7;

#[repr(C)]
pub struct SCNotification {
    pub h_wnd: *mut c_void,
    pub id_from: usize,
    pub code: u32,
    pub position: isize,
    pub ch: i32,
    pub modifiers: i32,
    pub modification_type: i32,
    pub text: *const i8,
    pub length: isize,
    pub lines_added: isize,
    pub message: i32,
    pub w_param: usize,
    pub l_param: isize,
    pub line: isize,
    pub fold_level_now: i32,
    pub fold_level_prev: i32,
    pub margin: i32,
    pub list_type: i32,
    pub x: i32,
    pub y: i32,
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn beNotified(notification: *mut SCNotification) {
    if notification.is_null() {
        return;
    }

    let code = unsafe { (*notification).code };
    if code == NPPN_FILESAVED {
        // In a real implementation on Windows, we'd get the current file path
        // and send a JSON-RPC request to localhost:4242.
        // For Phase 1/2 Cross-platform development, we logic-log this.
        info!("Notepad++: File Saved. Triggering RustShare Sync...");

        // RPC trigger placeholder — implement when Windows NPP integration is active
    }
}
