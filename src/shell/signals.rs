use tokio::{process::Child, sync::Mutex};

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const SIGTSTP: i32 = 20;
const SIGCONT: i32 = 18;

#[derive(Clone)]
pub struct SignalHandler {
    foreground_pid: Arc<Mutex<Option<u32>>>,
}

static CURRENT_FOREGROUND_PID: AtomicUsize = AtomicUsize::new(0);

extern "C" fn handle_tstp(_: libc::c_int) {
    unsafe {
        if let Some(pid) = std::ptr::NonNull::new(CURRENT_FOREGROUND_PID.load(Ordering::Relaxed) as *mut libc::c_void) {
            libc::kill(-(pid.as_ptr() as i32), SIGTSTP);
        }
    }
}

extern "C" fn handle_cont(_: libc::c_int) {
    unsafe {
        if let Some(pid) = std::ptr::NonNull::new(CURRENT_FOREGROUND_PID.load(Ordering::Relaxed) as *mut libc::c_void) {
            libc::kill(-(pid.as_ptr() as i32), SIGCONT);
        }
    }
}

impl SignalHandler {
    pub fn new() -> Self {
        unsafe {
            libc::signal(SIGTSTP, handle_tstp as libc::sighandler_t);
            libc::signal(SIGCONT, handle_cont as libc::sighandler_t);
        }
        Self {
            foreground_pid: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_foreground_process(&self, child: &Child) {
        let mut guard = self.foreground_pid.lock().await;
        *guard = child.id();
    }

    pub async fn clear_foreground_process(&self) {
        let mut guard = self.foreground_pid.lock().await;
        *guard = None;
    }

    pub fn update_foreground_pid(pid: Option<u32>) { CURRENT_FOREGROUND_PID.store(pid.unwrap_or(0) as usize, std::sync::atomic::Ordering::Relaxed); }
}
