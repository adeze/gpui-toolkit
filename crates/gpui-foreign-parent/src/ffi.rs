//! Null-safe FFI boundary helpers (contract §§4, 13).
//!
//! Every host/guest boundary crossing is total: null pointers are
//! rejected or treated as queries, strings are validated, and state
//! transfer is two-phase (query size with a null buffer, then fill).

use std::ffi::{CStr, c_char};

/// Copy a NUL-terminated C string; `None` for null or invalid UTF-8.
///
/// # Safety
///
/// `value` must be null or point to a valid NUL-terminated C string.
/// The bytes are copied out immediately.
pub unsafe fn optional_c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: non-null by the check above; the caller guarantees a valid
    // NUL-terminated string, and the bytes are copied out immediately.
    unsafe { CStr::from_ptr(value).to_str().ok().map(str::to_owned) }
}

/// Two-phase out-buffer fill; returns the total required size.
///
/// When `out` is null or `capacity` is 0, nothing is written and the
/// required size is returned. Otherwise up to `capacity` bytes are
/// written, `written` (when non-null) receives the count, and the return
/// value is the total required size — greater than `capacity` means the
/// caller's buffer was too small.
///
/// # Safety
///
/// `out` must be null or writable for `min(bytes.len(), capacity)`
/// bytes; `written` must be null or writable. Source and destination
/// must not overlap.
pub unsafe fn fill_out_buffer(
    bytes: &[u8],
    out: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> usize {
    let required = bytes.len();
    if !written.is_null() {
        // SAFETY: `written` is null-checked; the caller guarantees a
        // writable out count.
        unsafe {
            *written = 0;
        }
    }
    if out.is_null() || capacity == 0 {
        return required;
    }
    let count = required.min(capacity);
    // SAFETY: `out` is non-null with room for `count <= capacity`
    // bytes; source and destination are disjoint by contract.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, count);
        if !written.is_null() {
            *written = count;
        }
    }
    required
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn null_c_string_is_none() {
        // SAFETY: null is explicitly accepted.
        let value = unsafe { optional_c_string(std::ptr::null()) };
        assert!(value.is_none());
    }

    #[test]
    fn valid_c_string_is_copied() {
        let text = CString::new("cutoff").unwrap();
        // SAFETY: `text` is a live NUL-terminated string.
        let value = unsafe { optional_c_string(text.as_ptr()) };
        assert_eq!(value.as_deref(), Some("cutoff"));
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let bytes: [u8; 3] = [0x66, 0x80, 0x00];
        // SAFETY: NUL-terminated buffer, live for the call.
        let value = unsafe { optional_c_string(bytes.as_ptr().cast::<c_char>()) };
        assert!(value.is_none());
    }

    #[test]
    fn out_buffer_query_reports_size_without_writing() {
        let bytes = vec![1u8, 2, 3, 4];
        let mut written = 0usize;
        // SAFETY: null buffer is an explicit size query.
        let required =
            unsafe { fill_out_buffer(&bytes, std::ptr::null_mut(), 0, &raw mut written) };
        assert_eq!(required, 4);
        assert_eq!(written, 0);
    }

    #[test]
    fn out_buffer_fill_copies_and_reports() {
        let bytes = vec![1u8, 2, 3, 4];
        let mut out = [0u8; 8];
        let mut written = 0usize;
        // SAFETY: `out` has room for the full payload.
        let required =
            unsafe { fill_out_buffer(&bytes, out.as_mut_ptr(), out.len(), &raw mut written) };
        assert_eq!(required, 4);
        assert_eq!(written, 4);
        assert_eq!(&out[..4], &[1u8, 2, 3, 4]);
    }

    #[test]
    fn out_buffer_truncates_to_capacity() {
        let bytes = vec![1u8, 2, 3, 4];
        let mut out = [0u8; 2];
        let mut written = 0usize;
        // SAFETY: `out` has room for `capacity` bytes.
        let required =
            unsafe { fill_out_buffer(&bytes, out.as_mut_ptr(), out.len(), &raw mut written) };
        assert_eq!(required, 4);
        assert_eq!(written, 2);
        assert_eq!(out, [1u8, 2]);
    }

    #[test]
    fn out_buffer_accepts_null_written() {
        let bytes = vec![7u8];
        let mut out = [0u8; 1];
        // SAFETY: `out` has room; null `written` is explicitly accepted.
        let required =
            unsafe { fill_out_buffer(&bytes, out.as_mut_ptr(), out.len(), std::ptr::null_mut()) };
        assert_eq!(required, 1);
        assert_eq!(out, [7u8]);
    }
}
