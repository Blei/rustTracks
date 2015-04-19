// Misc util functions.

use std::borrow::ToOwned;
use std::ffi;
use std::str;

use libc;

// Please note that the lifetime of the returned string is a lie. It's probably safer to convert it
// to a String (or use ptr_to_string directly) if you want to do more than just display it
// immediately.
pub unsafe fn ptr_to_str(p: *const libc::c_char) -> &'static str {
    str::from_utf8(ffi::CStr::from_ptr(p).to_bytes()).unwrap()
}

pub unsafe fn ptr_to_string(p: *const libc::c_char) -> String {
    ptr_to_str(p).to_owned()
}
