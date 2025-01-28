use std::os::unix::io::AsRawFd;

#[link(name = "c")]
extern "C" {
    fn ttyname(fd: i32) -> *const libc::c_char;
    fn isatty(fd: i32) -> i32;
}

pub fn get_tty_name() -> Option<String> {
    let fd = std::io::stdin().as_raw_fd();
    unsafe {
        if isatty(fd) != 1 {
            return None;
        }
        let ptr = ttyname(fd);
        if ptr.is_null() {
            None
        } else {
            Some(std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
}

pub fn get_tty_name_or_default() -> String {
    get_tty_name()
        .map(|full_path| full_path.split('/').last().unwrap_or("ttys000").to_string())
        .unwrap_or_else(|| "ttys000".to_string())
}
