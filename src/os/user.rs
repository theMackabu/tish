use std::error::Error;
use std::ffi::CStr;
use std::os::raw::c_char;

extern "C" {
    fn getlogin() -> *const c_char;
    fn getpwuid(uid: u32) -> *const passwd;
    fn geteuid() -> u32;
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct passwd {
    pw_name: *const c_char,
    pw_passwd: *const c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_gecos: *const c_char,
    pw_dir: *const c_char,
    pw_shell: *const c_char,
}

pub fn get_username() -> Result<String, Box<dyn Error>> {
    let login = unsafe { getlogin() };
    if !login.is_null() {
        return Ok(unsafe { CStr::from_ptr(login) }.to_string_lossy().into_owned());
    }

    let uid = unsafe { geteuid() };
    let pw = unsafe { getpwuid(uid) };
    if pw.is_null() {
        return Err("Failed to get user information".into());
    }

    let username = unsafe { CStr::from_ptr((*pw).pw_name) };
    Ok(username.to_string_lossy().into_owned())
}
