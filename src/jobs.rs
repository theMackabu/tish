use std::{
    collections::HashMap,
    process::{ExitCode, Stdio},
};

use anyhow::{anyhow, Result};
use libc::id_t;
use tokio::process::{Child, Command};

pub struct JobManager(HashMap<id_t, Child>);

impl JobManager {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add_job(&mut self, handle: &mut Command) -> Result<ExitCode> {
        handle.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

        let mut child = handle.spawn()?;

        child.stdin.take();
        child.stdout.take();
        child.stderr.take();

        let pid = child.id().ok_or_else(|| anyhow!("Could not get process id"))?;
        self.0.insert(pid, child);

        Ok(ExitCode::SUCCESS)
    }

    pub fn contains_pid(&self, pid: id_t) -> bool {
        self.0.contains_key(&pid)
    }

    pub async fn remove_job(&mut self, pid: id_t) -> Result<ExitCode> {
        let job = self.0.get_mut(&pid).ok_or_else(|| anyhow!("kill: {}: No such process", pid))?;

        job.kill().await?;
        self.0.remove(&pid);

        Ok(ExitCode::SUCCESS)
    }

    pub fn list_jobs(&mut self) -> Result<ExitCode> {
        let mut completed_pids = Vec::new();

        for (pid, job) in &mut self.0 {
            match job.try_wait()? {
                Some(status) => {
                    println!("PID {}: Completed with status {}", pid, status);
                    completed_pids.push(*pid);
                }
                None => println!("PID {}: Still running", pid),
            }
        }

        for pid in completed_pids {
            self.0.remove(&pid);
        }

        Ok(ExitCode::SUCCESS)
    }
}
