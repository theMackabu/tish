use std::ffi::{CStr, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use super::{
    user::{Group, User},
    GroupExtras, UserExtras,
};

pub unsafe fn from_raw_buf<'a, T>(p: *const libc::c_char) -> T
where
    T: From<&'a OsStr>,
{
    T::from(OsStr::from_bytes(CStr::from_ptr(p).to_bytes()))
}

pub unsafe fn members(groups: *mut *mut libc::c_char) -> Vec<OsString> {
    let mut members = Vec::new();

    for i in 0.. {
        let username = groups.offset(i);

        if username.is_null() || (*username).is_null() {
            break;
        } else {
            members.push(from_raw_buf(*username));
        }
    }

    members
}

pub unsafe fn passwd_to_user(passwd: libc::passwd) -> User {
    let name = from_raw_buf(passwd.pw_name);

    User {
        uid: passwd.pw_uid,
        name_arc: name,
        primary_group: passwd.pw_gid,
        extras: UserExtras::from_passwd(passwd),
    }
}

pub unsafe fn struct_to_group(group: libc::group) -> Group {
    let name = from_raw_buf(group.gr_name);

    Group {
        gid: group.gr_gid,
        name_arc: name,
        extras: GroupExtras::from_struct(group),
    }
}

#[macro_export]
macro_rules! env_set_sync {
    ( $( $key:tt = $val:expr ),* $(,)? ) => {{
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap();

        $(
            let key = match stringify!($key).parse::<u64>() {
                Ok(_) => stringify!($key),
                Err(_) => stringify!($key),
            };
            unsafe { std::env::set_var(key, $val); }
        )*
    }};
}
