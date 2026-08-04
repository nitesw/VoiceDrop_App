use std::ffi::CString;
use std::os::raw::c_char;

/// Returns a fixed greeting. Used only to prove the Swift-to-Rust FFI
/// round-trip works before any real pipeline logic exists.
fn ping() -> String {
    "pong from voicedrop-core".to_string()
}

/// C ABI entry point: returns an owned, NUL-terminated C string.
/// Callers must free the result with `voicedrop_core_free_string`.
#[no_mangle]
pub extern "C" fn voicedrop_core_ping() -> *mut c_char {
    CString::new(ping())
        .expect("ping() output must not contain interior NUL bytes")
        .into_raw()
}

/// Frees a string previously returned by `voicedrop_core_ping`.
#[no_mangle]
pub extern "C" fn voicedrop_core_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn ping_returns_expected_text() {
        assert_eq!(ping(), "pong from voicedrop-core");
    }

    #[test]
    fn ffi_round_trip_returns_same_text() {
        let ptr = voicedrop_core_ping();
        let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        assert_eq!(text, ping());
        voicedrop_core_free_string(ptr);
    }
}
