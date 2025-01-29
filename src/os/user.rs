#![allow(dead_code)]

use libc::{c_char, c_int, gid_t, group as c_group, passwd as c_passwd, uid_t};

use std::{
    error::Error,
    ffi::{CStr, CString, OsStr},
    mem,
    os::unix::ffi::OsStrExt,
    ptr,
    sync::Arc,
};

extern "C" {
    fn getpwuid(uid: u32) -> *const passwd;
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct passwd {
    pub pw_name: *const c_char,
}

#[derive(Clone)]
pub struct User {
    pub uid: uid_t,
    pub primary_group: gid_t,
    pub extras: super::UserExtras,
    pub(crate) name_arc: Arc<OsStr>,
}

#[derive(Clone)]
pub struct Group {
    pub gid: gid_t,
    pub extras: super::GroupExtras,
    pub(crate) name_arc: Arc<OsStr>,
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

pub fn get_user_groups<S: AsRef<OsStr> + ?Sized>(username: &S, gid: gid_t) -> Option<Vec<Group>> {
    #[cfg(all(unix, target_os = "macos"))]
    let mut buff: Vec<i32> = vec![0; 1024];
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut buff: Vec<gid_t> = vec![0; 1024];

    let name = CString::new(username.as_ref().as_bytes()).unwrap();
    let mut count = buff.len() as c_int;

    #[cfg(all(unix, target_os = "macos"))]
    let res = unsafe { libc::getgrouplist(name.as_ptr(), gid as i32, buff.as_mut_ptr(), &mut count) };
    #[cfg(all(unix, not(target_os = "macos")))]
    let res = unsafe { libc::getgrouplist(name.as_ptr(), gid, buff.as_mut_ptr(), &mut count) };

    if res < 0 {
        None
    } else {
        buff.dedup();
        buff.into_iter().filter_map(|i| get_group_by_gid(i as gid_t)).collect::<Vec<_>>().into()
    }
}

pub fn get_user_by_uid(uid: uid_t) -> Option<User> {
    let mut buf = vec![0; 2048];
    let mut passwd = unsafe { mem::zeroed::<c_passwd>() };
    let mut result = ptr::null_mut::<c_passwd>();

    loop {
        let r = unsafe { libc::getpwuid_r(uid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };
        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        return None;
    }

    if result != &mut passwd {
        return None;
    }

    let user = unsafe { super::r#unsafe::passwd_to_user(result.read()) };
    Some(user)
}

pub fn get_group_by_gid(gid: gid_t) -> Option<Group> {
    let mut buf = vec![0; 2048];
    let mut passwd = unsafe { mem::zeroed::<c_group>() };
    let mut result = ptr::null_mut::<c_group>();

    loop {
        let r = unsafe { libc::getgrgid_r(gid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };
        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        return None;
    }

    if result != &mut passwd {
        return None;
    }

    let group = unsafe { super::r#unsafe::struct_to_group(result.read()) };
    Some(group)
}

impl User {
    pub fn new<S: AsRef<OsStr> + ?Sized>(uid: uid_t, name: &S, primary_group: gid_t) -> Self {
        let name_arc = Arc::from(name.as_ref());
        let extras = super::UserExtras::default();

        Self { uid, name_arc, primary_group, extras }
    }

    pub fn uid(&self) -> uid_t { self.uid }

    pub fn name(&self) -> &OsStr { &*self.name_arc }

    pub fn primary_group_id(&self) -> gid_t { self.primary_group }

    pub fn groups(&self) -> Option<Vec<Group>> { get_user_groups(self.name(), self.primary_group_id()) }
}
