use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::{process::Child, sync::Mutex};

const SIGTSTP: i32 = 20;
const SIGCONT: i32 = 18;

static CURRENT_FOREGROUND_PID: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_SIGNAL_HANDLER: OnceLock<Arc<SignalHandler>> = OnceLock::new();

#[derive(Clone)]
pub struct SignalHandler {
    foreground_pid: Arc<Mutex<Option<u32>>>,
    foreground_info: Arc<StdMutex<Option<(String, Vec<String>)>>>,
}

impl SignalHandler {
    pub fn new() -> Self {
        let handler = Self {
            foreground_pid: Arc::new(Mutex::new(None)),
            foreground_info: Arc::new(StdMutex::new(None)),
        };

        let arc_handler = Arc::new(handler.clone());
        let _ = GLOBAL_SIGNAL_HANDLER.get_or_init(|| arc_handler.clone());

        unsafe {
            libc::signal(SIGTSTP, handle_tstp as libc::sighandler_t);
            libc::signal(SIGCONT, handle_cont as libc::sighandler_t);
        }

        handler
    }

    pub fn get_foreground_info(&self) -> Option<(String, Vec<String>)> { self.foreground_info.lock().ok()?.clone() }

    pub async fn set_foreground_process(&self, child: &Child, program: &str, args: &[String]) {
        let mut pid_guard = self.foreground_pid.lock().await;
        *pid_guard = child.id();

        if let Ok(mut info_guard) = self.foreground_info.lock() {
            *info_guard = Some((program.to_string(), args.to_vec()));
        }
    }

    pub async fn clear_foreground_process(&self) {
        let mut pid_guard = self.foreground_pid.lock().await;
        *pid_guard = None;

        if let Ok(mut info_guard) = self.foreground_info.lock() {
            *info_guard = None;
        }
    }

    pub fn update_foreground_pid(pid: Option<u32>) { CURRENT_FOREGROUND_PID.store(pid.unwrap_or(0) as usize, Ordering::SeqCst); }
}

extern "C" fn handle_tstp(_: libc::c_int) {
    unsafe {
        if let Some(pid) = std::ptr::NonNull::new(CURRENT_FOREGROUND_PID.load(Ordering::SeqCst) as *mut libc::c_void) {
            let pid_val = pid.as_ptr() as i32;
            libc::kill(-pid_val, SIGTSTP);

            let shell_pgid = libc::getpgrp();
            libc::tcsetpgrp(0, shell_pgid);
        }
    }
}

extern "C" fn handle_cont(_: libc::c_int) {
    unsafe {
        if let Some(pid) = std::ptr::NonNull::new(CURRENT_FOREGROUND_PID.load(Ordering::SeqCst) as *mut libc::c_void) {
            let pid_val = pid.as_ptr() as i32;
            libc::kill(-pid_val, SIGCONT);
            libc::tcsetpgrp(0, pid_val);
        }
    }
}
