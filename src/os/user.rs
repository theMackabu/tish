use std::error::Error;
use std::ffi::CStr;
use std::os::raw::c_char;

extern "C" {
    fn getpwuid(uid: u32) -> *const passwd;
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct passwd {
    pw_name: *const c_char,
}

pub fn get_username() -> Result<String, Box<dyn Error>> {
    let uid = unsafe { libc::getuid() };
    let pw = unsafe { getpwuid(uid) };

    if pw.is_null() {
        return Err("Failed to get user information".into());
    }

    let username = unsafe { CStr::from_ptr((*pw).pw_name) };
    Ok(username.to_string_lossy().into_owned())
}
