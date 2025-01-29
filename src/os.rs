#![allow(dead_code)]

pub mod env;
pub mod r#unsafe;
pub mod user;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "solaris"
))]
pub mod unix {
    use super::r#unsafe::from_raw_buf;
    use libc::{group as c_group, passwd as c_passwd};

    use std::{
        ffi::{OsStr, OsString},
        path::{Path, PathBuf},
    };

    pub trait UserExt {
        fn home_dir(&self) -> &Path;
        fn with_home_dir<S: AsRef<OsStr> + ?Sized>(self, home_dir: &S) -> Self;
        fn shell(&self) -> &Path;
        fn with_shell<S: AsRef<OsStr> + ?Sized>(self, shell: &S) -> Self;
        fn password(&self) -> &OsStr;
        fn with_password<S: AsRef<OsStr> + ?Sized>(self, password: &S) -> Self;
    }

    pub trait GroupExt {
        fn members(&self) -> &[OsString];
        fn add_member<S: AsRef<OsStr> + ?Sized>(self, name: &S) -> Self;
    }

    #[derive(Clone, Debug)]
    pub struct UserExtras {
        pub home_dir: PathBuf,
        pub shell: PathBuf,
        pub password: OsString,
    }

    impl Default for UserExtras {
        fn default() -> Self {
            Self {
                home_dir: "/var/empty".into(),
                shell: "/bin/false".into(),
                password: "*".into(),
            }
        }
    }

    impl UserExtras {
        pub(crate) unsafe fn from_passwd(passwd: c_passwd) -> Self {
            #[cfg(target_os = "android")]
            {
                Default::default()
            }
            #[cfg(not(target_os = "android"))]
            {
                let home_dir = from_raw_buf::<OsString>(passwd.pw_dir).into();
                let shell = from_raw_buf::<OsString>(passwd.pw_shell).into();
                let password = from_raw_buf::<OsString>(passwd.pw_passwd);

                Self { home_dir, shell, password }
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    use super::user::User;

    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    impl UserExt for User {
        fn home_dir(&self) -> &Path { Path::new(&self.extras.home_dir) }

        fn with_home_dir<S: AsRef<OsStr> + ?Sized>(mut self, home_dir: &S) -> Self {
            self.extras.home_dir = home_dir.into();
            self
        }

        fn shell(&self) -> &Path { Path::new(&self.extras.shell) }

        fn with_shell<S: AsRef<OsStr> + ?Sized>(mut self, shell: &S) -> Self {
            self.extras.shell = shell.into();
            self
        }

        fn password(&self) -> &OsStr { &self.extras.password }

        fn with_password<S: AsRef<OsStr> + ?Sized>(mut self, password: &S) -> Self {
            self.extras.password = password.into();
            self
        }
    }

    #[derive(Clone, Default, Debug)]
    pub struct GroupExtras {
        pub members: Vec<OsString>,
    }

    impl GroupExtras {
        pub(crate) unsafe fn from_struct(group: c_group) -> Self {
            Self {
                members: super::r#unsafe::members(group.gr_mem),
            }
        }
    }

    impl GroupExt for super::user::Group {
        fn members(&self) -> &[OsString] { &*self.extras.members }

        fn add_member<S: AsRef<OsStr> + ?Sized>(mut self, member: &S) -> Self {
            self.extras.members.push(member.into());
            self
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "dragonfly", target_os = "openbsd", target_os = "netbsd"))]
pub mod bsd {
    use super::user::User;
    use libc::passwd;
    use libc::time_t;
    use std::ffi::OsStr;
    use std::path::Path;

    #[derive(Clone, Debug)]
    pub struct UserExtras {
        pub extras: super::unix::UserExtras,
        pub change: time_t,
        pub expire: time_t,
    }

    impl UserExtras {
        pub(crate) unsafe fn from_passwd(passwd: passwd) -> Self {
            Self {
                change: passwd.pw_change,
                expire: passwd.pw_expire,
                extras: super::unix::UserExtras::from_passwd(passwd),
            }
        }
    }

    impl super::unix::UserExt for User {
        fn home_dir(&self) -> &Path { Path::new(&self.extras.extras.home_dir) }

        fn with_home_dir<S: AsRef<OsStr> + ?Sized>(mut self, home_dir: &S) -> Self {
            self.extras.extras.home_dir = home_dir.into();
            self
        }

        fn shell(&self) -> &Path { Path::new(&self.extras.extras.shell) }

        fn with_shell<S: AsRef<OsStr> + ?Sized>(mut self, shell: &S) -> Self {
            self.extras.extras.shell = shell.into();
            self
        }

        fn password(&self) -> &OsStr { &self.extras.extras.password }

        fn with_password<S: AsRef<OsStr> + ?Sized>(mut self, password: &S) -> Self {
            self.extras.extras.password = password.into();
            self
        }
    }

    pub trait UserExt {
        fn password_change_time(&self) -> time_t;
        fn password_expire_time(&self) -> time_t;
    }

    impl UserExt for User {
        fn password_change_time(&self) -> time_t { self.extras.change }
        fn password_expire_time(&self) -> time_t { self.extras.expire }
    }

    impl Default for UserExtras {
        fn default() -> Self {
            Self {
                extras: super::unix::UserExtras::default(),
                change: 0,
                expire: 0,
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "dragonfly", target_os = "openbsd", target_os = "netbsd"))]
pub type UserExtras = bsd::UserExtras;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
pub type UserExtras = unix::UserExtras;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "solaris"
))]
pub type GroupExtras = unix::GroupExtras;
