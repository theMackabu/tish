use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::process::Child;

pub const SIGTSTP: i32 = 20;
pub const SIGCONT: i32 = 18;
pub const SIGINT: i32 = 2;

pub(crate) static CURRENT_FOREGROUND_PID: AtomicI32 = AtomicI32::new(-1);
pub(crate) static GLOBAL_SIGNAL_HANDLER: OnceLock<Arc<SignalHandler>> = OnceLock::new();

#[derive(Clone)]
pub struct SignalHandler {
    pub foreground_info: Arc<Mutex<Option<(String, Vec<String>)>>>,
}

impl SignalHandler {
    pub fn new() -> Self {
        let handler = Self {
            foreground_info: Arc::new(Mutex::new(None)),
        };

        let arc_handler = Arc::new(handler.clone());
        let _ = GLOBAL_SIGNAL_HANDLER.get_or_init(|| arc_handler.clone());

        unsafe {
            libc::signal(SIGTSTP, handle_tstp as libc::sighandler_t);
            libc::signal(SIGCONT, handle_cont as libc::sighandler_t);
            libc::signal(SIGINT, handle_int as libc::sighandler_t);
        }

        handler
    }

    pub async fn set_foreground_process(&self, child: &Child, program: &str, args: &[String]) {
        let pid = child.id().unwrap_or(0) as i32;
        CURRENT_FOREGROUND_PID.store(pid, Ordering::SeqCst);

        if let Ok(mut info_guard) = self.foreground_info.lock() {
            *info_guard = Some((program.to_string(), args.to_vec()));
        }
    }

    pub async fn clear_foreground_process(&self) {
        CURRENT_FOREGROUND_PID.store(-1, Ordering::SeqCst);

        if let Ok(mut info_guard) = self.foreground_info.lock() {
            *info_guard = None;
        }
    }
}

extern "C" fn handle_tstp(_: libc::c_int) {
    unsafe {
        let pid = CURRENT_FOREGROUND_PID.load(Ordering::SeqCst);
        if pid <= 0 {
            return;
        }

        if libc::kill(-pid, SIGTSTP) == 0 {
            let shell_pgid = libc::getpgrp();
            libc::tcsetpgrp(0, shell_pgid);

            if let Some(handler) = GLOBAL_SIGNAL_HANDLER.get() {
                if let Ok(info) = handler.foreground_info.lock() {
                    if let Some((cmd, args)) = info.as_ref() {
                        if let Ok(mut jobs) = crate::JOBS.try_lock() {
                            jobs.suspend_job(pid as u32, cmd, args);
                        }
                    }
                }
            }
        }

        CURRENT_FOREGROUND_PID.store(-1, Ordering::SeqCst);
    }
}

extern "C" fn handle_int(_: libc::c_int) {
    unsafe {
        let pid = CURRENT_FOREGROUND_PID.load(Ordering::SeqCst);
        if pid > 0 {
            libc::kill(-pid, SIGINT);
            let shell_pgid = libc::getpgrp();
            libc::tcsetpgrp(0, shell_pgid);
        }
        CURRENT_FOREGROUND_PID.store(-1, Ordering::SeqCst);
    }
}

pub extern "C" fn handle_cont(_: libc::c_int) {
    unsafe {
        let pid = CURRENT_FOREGROUND_PID.load(Ordering::SeqCst);
        if pid > 0 {
            libc::kill(-pid, SIGCONT);
            libc::tcsetpgrp(0, pid);
        }

        libc::signal(SIGCONT, handle_cont as libc::sighandler_t);
    }
}
