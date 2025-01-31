pub use super::lua::LuaState;

use crate::{
    cmd,
    models::{Command, InternalCommand},
    os::env::EnvManager,
    shell::{signals::*, tokenizer::Tokenizer, TishShell},
};

use anyhow::{anyhow, Result};
use tokio::task;

use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::Ordering,
};

pub struct TishCommand {
    args: Vec<String>,
    background: bool,

    pub program: String,
    pub pipe_to: Option<Box<TishCommand>>,
    pub redirect_in: Option<String>,
    pub redirect_out: Option<(String, bool)>,
}

impl TishCommand {
    pub fn parse(input: &str) -> Vec<Self> {
        if input.trim().is_empty() {
            return vec![];
        }

        let parse_command = |cmd_str: &str| -> Option<Self> {
            let expanded = EnvManager::new(cmd_str).expand();

            if expanded.contains('|') {
                let parts: Vec<&str> = expanded.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();

                let mut final_cmd = None;
                for part in parts.into_iter().rev() {
                    let mut current_cmd = Self::parse_single_command(Tokenizer::new(part));
                    if let Some(next_cmd) = final_cmd {
                        current_cmd.pipe_to = Some(Box::new(next_cmd));
                    }
                    final_cmd = Some(current_cmd);
                }
                final_cmd
            } else {
                Some(Self::parse_single_command(Tokenizer::new(&expanded)))
            }
        };

        input.split("&&").map(str::trim).filter(|s| !s.is_empty()).filter_map(parse_command).collect()
    }

    pub async fn execute(&self, shell: &TishShell) -> Result<ExitCode> {
        let command = Command::from_str(&self.program, &self.args);
        let internal_command = InternalCommand::from_str(&self.program, &self.args);

        if self.program.as_str() == "tish" && self.args.len() != 0 {
            let result = match internal_command {
                InternalCommand::Fg => self.handle_builtin_fg().await?,
                InternalCommand::Jobs => crate::JOBS.lock().expect("Able to lock jobs").list_jobs().await?,
                InternalCommand::Help => Self::handle_builtin_help()?,
                InternalCommand::Kill => self.handle_builtin_kill().await?,
                InternalCommand::External => self.execute_external(shell).await?,
                InternalCommand::Script => shell.lua.eval_file(std::path::Path::new(&self.program))?,

                InternalCommand::Pid => {
                    println!("{}", std::process::id());
                    return Ok(ExitCode::SUCCESS);
                }
            };

            return Ok(result);
        }

        let result = match command {
            Command::Fg => self.handle_builtin_fg().await?,
            Command::Cd => self.handle_builtin_cd()?,
            Command::Help => Self::handle_builtin_help()?,
            Command::Jobs => crate::JOBS.lock().expect("Able to lock jobs").list_jobs().await?,
            Command::External => self.execute_external(shell).await?,
            Command::Script => shell.lua.eval_file(std::path::Path::new(&self.program))?,
            Command::Source => shell.lua.eval_file(Path::new(&self.args.get(0).ok_or_else(|| anyhow!("Could not determine source file"))?))?,

            Command::Ls => match shell.lua.get_config_value("use_tish_ls")? {
                true => cmd::ls::run(&self.args)?,
                false => self.execute_external(shell).await?,
            },

            Command::Exit => {
                CURRENT_FOREGROUND_PID.store(-1, Ordering::SeqCst);

                if let Some(handler) = GLOBAL_SIGNAL_HANDLER.get() {
                    if let Ok(mut info_guard) = handler.foreground_info.lock() {
                        *info_guard = None;
                    }
                }

                unsafe {
                    libc::signal(SIGTSTP, libc::SIG_DFL);
                    libc::signal(SIGCONT, libc::SIG_DFL);
                    libc::signal(SIGINT, libc::SIG_DFL);
                }

                std::process::exit(0);
            }
        };

        Ok(result)
    }

    async fn execute_external(&self, shell: &TishShell) -> Result<ExitCode> {
        let auto_cd = shell.lua.get_config_value("auto_cd")?;

        let path_str = if self.program.starts_with("~/") {
            dirs::home_dir()
                .map(|mut p| {
                    p.push(&self.program[2..]);
                    p
                })
                .unwrap_or_else(|| PathBuf::from(&self.program))
        } else {
            PathBuf::from(&self.program)
        };

        if auto_cd && path_str.is_dir() {
            return TishCommand {
                program: "cd".to_string(),
                args: vec![path_str.to_string_lossy().into_owned()],
                background: false,
                pipe_to: None,
                redirect_in: None,
                redirect_out: None,
            }
            .handle_builtin_cd();
        }

        if self.background {
            self.spawn_background_job()?;
            Ok(ExitCode::SUCCESS)
        } else {
            self.spawn_foreground_job(&shell.signal_handler).await
        }
    }

    fn spawn_background_job(&self) -> Result<()> {
        let program = self.program.clone();
        let args = self.args.clone();

        task::spawn(async move {
            let mut handle = tokio::process::Command::new(&program);
            handle.args(&args);

            if let Ok(mut manager) = crate::JOBS.try_lock() {
                if let Err(err) = manager.add_job(&mut handle, program, args) {
                    eprintln!("Failed to add background job: {err}");
                } else {
                    if let Some(job) = manager.jobs.values().last() {
                        println!("[{}] {}", job.id, job.pid);
                    }
                }
            } else {
                eprintln!("Failed to acquire jobs lock for background process");
            }
        });

        Ok(())
    }

    async fn spawn_foreground_job(&self, signal_handler: &SignalHandler) -> Result<ExitCode> {
        let command = self.resolve_command();

        let mut cmd = tokio::process::Command::new(&command[0].program);
        cmd.args(&command[0].args).args(&self.args);

        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                libc::signal(SIGTSTP, libc::SIG_DFL);
                libc::signal(SIGINT, libc::SIG_DFL);
                libc::signal(SIGCONT, libc::SIG_DFL);

                Ok(())
            });
        }

        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0) as i32;

        unsafe {
            std::thread::sleep(std::time::Duration::from_millis(1));
            if libc::tcsetpgrp(0, pid) != 0 {
                eprintln!("Failed to set terminal foreground process group");
            }
        }

        CURRENT_FOREGROUND_PID.store(pid, Ordering::SeqCst);
        signal_handler.set_foreground_process(&child, &self.program, &self.args).await;
        let status = child.wait().await?;

        unsafe {
            let shell_pgid = libc::getpgrp();
            if libc::tcsetpgrp(0, shell_pgid) != 0 {
                eprintln!("Failed to return terminal control to shell");
            }
        }

        CURRENT_FOREGROUND_PID.store(-1, Ordering::SeqCst);
        signal_handler.clear_foreground_process().await;

        Ok(ExitCode::from(status.code().unwrap_or(0) as u8))
    }

    fn handle_builtin_help() -> Result<ExitCode> {
        println!(
            concat!(
                "TISH, version {}-release\n",
                "These shell commands are defined internally. Type `help' to see this list.\n\n",
                "  tish jobs           - List background jobs\n",
                "  tish kill           - Kill a background job\n",
                "  tish pid            - Get current shell process id\n",
                "  source              - Source a file for env\n",
                "  help, ?             - Show this message\n",
                "  exit                - Exit TISH shell\n\n",
                "  *.lua               - Execute Lua script\n",
                "  lua_code            - Execute Lua code directly"
            ),
            env!("CARGO_PKG_VERSION")
        );
        Ok(ExitCode::SUCCESS)
    }

    async fn handle_builtin_fg(&self) -> Result<ExitCode> {
        let job_id = self.args.get(1).and_then(|s| s.parse::<usize>().ok());

        let pid = match crate::JOBS.try_lock() {
            Ok(mut jobs) => jobs.resume_job(job_id),
            Err(_) => return Err(anyhow!("fg: unable to acquire jobs lock")),
        };

        match pid {
            Some(pid) => {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGCONT);
                    libc::tcsetpgrp(0, pid as i32);
                }

                let status = tokio::process::Command::new("wait").arg(pid.to_string()).status().await?;

                unsafe {
                    let shell_pgid = libc::getpgrp();
                    libc::tcsetpgrp(0, shell_pgid);
                }

                Ok(ExitCode::from(status.code().unwrap_or(0) as u8))
            }
            None => Err(anyhow!("no current job")),
        }
    }

    async fn handle_builtin_kill(&self) -> Result<ExitCode> {
        let pid = match self.args.get(0) {
            Some(cmd) if cmd == "kill" => self.args.get(1).ok_or_else(|| anyhow!("kill: no process id specified"))?.parse()?,
            Some(_) => return Err(anyhow!("kill: invalid command")),
            None => return Err(anyhow!("kill: no command specified")),
        };

        let job_exists = match crate::JOBS.try_lock() {
            Ok(jobs) => jobs.contains_pid(pid),
            Err(_) => return Err(anyhow!("kill: unable to acquire lock, try again later")),
        };

        if !job_exists {
            return Err(anyhow!("illegal process id: {}", pid));
        }

        match crate::JOBS.try_lock() {
            Ok(mut jobs) => jobs.remove_job(pid).await?,
            Err(_) => return Err(anyhow!("kill: unable to acquire lock, try again later")),
        };

        Ok(ExitCode::SUCCESS)
    }

    fn handle_builtin_cd(&self) -> Result<ExitCode> {
        let target_dir = if self.args.is_empty() {
            dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?
        } else {
            PathBuf::from(&self.args[0])
        };

        env::set_current_dir(&target_dir).map_err(|_| anyhow!("cd: no such file or directory: {}", target_dir.display()))?;
        Ok(ExitCode::SUCCESS)
    }

    fn resolve_command(&self) -> Vec<Self> {
        let alias = crate::ALIASES.lock().expect("Able to acquire alias lock");
        let line = alias.get(&self.program).map(String::to_owned).unwrap_or_else(|| self.program.to_owned());

        TishCommand::parse(&line)
    }

    fn parse_single_command(mut tokenizer: Tokenizer) -> Self {
        let tokens = if tokenizer.has_redirection() { tokenizer.args_before_redirection() } else { tokenizer.get_args() };

        if tokens.is_empty() {
            return Self {
                program: String::new(),
                args: Vec::new(),
                background: false,
                pipe_to: None,
                redirect_in: None,
                redirect_out: None,
            };
        }

        let program = tokens[0].clone();
        let args = tokens[1..].to_vec();

        let background = args.last().map_or(false, |last| last == "&");
        let args = if background { args[..args.len() - 1].to_vec() } else { args };

        let mut redirect_in = None;
        let mut redirect_out = None;

        while !tokenizer.is_empty() {
            match tokenizer.next() {
                Some(op) if op == "<" => {
                    if let Some(file) = tokenizer.next() {
                        redirect_in = Some(file);
                    }
                }
                Some(op) if op == ">" => {
                    if let Some(file) = tokenizer.next() {
                        redirect_out = Some((file, false));
                    }
                }
                Some(op) if op == ">>" => {
                    if let Some(file) = tokenizer.next() {
                        redirect_out = Some((file, true));
                    }
                }
                _ => {}
            }
        }

        Self {
            program,
            args,
            background,
            pipe_to: None,
            redirect_in,
            redirect_out,
        }
    }
}
