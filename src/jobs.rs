use std::{
    collections::HashMap,
    process::{ExitCode, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

use nix::{
    errno::Errno,
    sys::signal::{kill, Signal},
    unistd::Pid,
};

use anyhow::{anyhow, Result};
use libc::id_t;
use tokio::process::Command;

#[derive(Debug)]
pub enum JobStatus {
    Running,
    Suspended,
    Completed(i32),
}

#[derive(Debug)]
pub struct Job {
    pub id: usize,
    pub pid: id_t,
    pub status: JobStatus,
    pub command: String,
    pub args: Vec<String>,
}

pub struct JobManager {
    pub jobs: HashMap<id_t, Job>,
    job_counter: AtomicUsize,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            job_counter: AtomicUsize::new(1),
        }
    }

    pub fn add_job(&mut self, handle: &mut Command, command: String, args: Vec<String>) -> Result<ExitCode> {
        handle.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

        let mut child = handle.spawn()?;
        child.stdin.take();
        child.stdout.take();
        child.stderr.take();

        let pid = child.id().ok_or_else(|| anyhow!("Could not get process id"))?;
        let job_id = self.job_counter.fetch_add(1, Ordering::SeqCst);

        self.jobs.insert(
            pid,
            Job {
                id: job_id,
                pid,
                args,
                command,
                status: JobStatus::Running,
            },
        );

        Ok(ExitCode::SUCCESS)
    }

    pub fn contains_pid(&self, pid: id_t) -> bool { self.jobs.contains_key(&pid) }

    pub fn get_job_by_id(&self, job_id: usize) -> Option<&Job> { self.jobs.values().find(|job| job.id == job_id) }

    pub fn get_last_suspended(&self) -> Option<&Job> { self.jobs.values().filter(|job| matches!(job.status, JobStatus::Suspended)).max_by_key(|job| job.id) }

    pub async fn remove_job(&mut self, pid: id_t) -> Result<ExitCode> {
        let job = self.jobs.get_mut(&pid).ok_or_else(|| anyhow!("kill: {}: No such process", pid))?;
        let i32_pid: i32 = pid.try_into().map_err(|_| anyhow!("PID too large"))?;

        match kill(Pid::from_raw(i32_pid), Signal::SIGTERM) {
            Ok(_) => (),
            Err(err) => match err {
                Errno::ESRCH => {
                    // just clean up our internal state
                }
                _ => kill(Pid::from_raw(i32_pid), Signal::SIGKILL).map_err(|err| anyhow!("Failed to kill process {}: {}", pid, err))?,
            },
        }

        job.status = JobStatus::Completed(0);
        self.jobs.remove(&pid);

        Ok(ExitCode::SUCCESS)
    }

    pub fn suspend_job(&mut self, pid: id_t) {
        if let Some(job) = self.jobs.get_mut(&pid) {
            job.status = JobStatus::Suspended;
            println!("[{}] tish: suspended {} {}", job.id, job.command, job.args.join(" "));
        }
    }

    pub async fn list_jobs(&mut self) -> Result<ExitCode> {
        let mut completed_pids = Vec::new();

        for job in self.jobs.values() {
            let i32_pid: i32 = job.pid.try_into().map_err(|_| anyhow!("PID too large"))?;
            let is_running = kill(Pid::from_raw(i32_pid), None).is_ok();

            let status_str = match job.status {
                JobStatus::Running => match is_running {
                    false => {
                        completed_pids.push(job.pid);
                        "Completed"
                    }
                    true => "Running",
                },
                JobStatus::Suspended => match is_running {
                    false => {
                        completed_pids.push(job.pid);
                        "Completed"
                    }
                    true => "Suspended",
                },
                JobStatus::Completed(code) => {
                    completed_pids.push(job.pid);
                    return Ok(ExitCode::from(code as u8));
                }
            };

            println!("[{}] {} {} {}", job.id, status_str, job.command, job.args.join(" "));
        }

        for pid in completed_pids {
            self.remove_job(pid).await?;
        }

        Ok(ExitCode::SUCCESS)
    }

    pub fn resume_job(&mut self, job_id: Option<usize>) -> Option<id_t> {
        let job = match job_id {
            Some(id) => self.jobs.values().find(|j| j.id == id),
            None => self.get_last_suspended(),
        }?;

        let pid = job.pid;
        if let Some(job) = self.jobs.get_mut(&pid) {
            job.status = JobStatus::Running;
        }
        Some(pid)
    }
}
