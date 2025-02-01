pub mod git;
pub mod highlight;
pub mod signals;
pub mod tokenizer;

use crate::{
    args::TishArgs,
    command::{LuaState, TishCommand},
    os::{env::EnvManager, user},
    prelude::*,
    readline::AsyncLineReader,
    template::Template,
    tty::get_tty_name_or_default,
};

use std::{
    env,
    path::PathBuf,
    process::{self, ExitCode},
};

use anyhow::Result;
use chrono::{DateTime, Local};
use rustyline::error::ReadlineError;
use signals::SignalHandler;

pub struct TishShell {
    pub args: TishArgs,
    pub lua: LuaState,
    pub home: Option<PathBuf>,
    pub signal_handler: SignalHandler,

    readline: AsyncLineReader,
}

impl TishShell {
    pub async fn new(args: TishArgs) -> Result<Self> {
        unsafe {
            let shell_pid = libc::getpid();
            if libc::setpgid(shell_pid, shell_pid) != 0 {
                eprintln!("Failed to set shell process group");
            }

            if libc::tcsetpgrp(0, shell_pid) != 0 {
                eprintln!("Failed to set initial terminal control");
            }

            libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            libc::signal(libc::SIGTTIN, libc::SIG_IGN);
        }

        let mut shell = Self {
            args: args.to_owned(),
            lua: LuaState::new()?,
            home: dirs::home_dir(),
            readline: AsyncLineReader::new()?,
            signal_handler: SignalHandler::new(),
        };

        if !args.no_env {
            shell.load_config()?;
        }

        if args.login {
            shell.load_profile()?;
        }

        if !args.headless {
            shell.login_message()?;
        }

        if let Some(line) = args.arguments {
            if let Err(_) = shell.lua.eval(&line) {
                let status = shell.execute_command(&line).await;
                let raw_code = unsafe { std::mem::transmute::<ExitCode, u8>(status) };
                process::exit(raw_code as i32);
            }
        }

        Ok(shell)
    }

    fn login_message(&self) -> Result<ExitCode> {
        dotfile! {
            not, self.home => ".hushlogin",
            |_| {
                let tty = get_tty_name_or_default();
                let now: DateTime<Local> = Local::now();
                let formatted_date = now.format("%a %b %d %H:%M:%S").to_string();
                println!("Last login: {} on {}", formatted_date, tty);
            }
        }
    }

    fn load_config(&self) -> Result<ExitCode> {
        dotfile! {
            self.home => ".tishrc",
            |config| self.lua.eval_file(config)
        }
    }

    fn load_profile(&self) -> Result<ExitCode> {
        dotfile! {
            self.home => ".tish_profile",
            |profile| self.lua.eval_file(profile)
        }
    }

    fn format_prompt(&self) -> Result<String> {
        let str: String = self.lua.get_config_value("prompt")?;
        let host = hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
        let path = env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();

        let tmpl = Template::new(&str);
        let envm = EnvManager::new(&path);
        let git_info = git::get_info();

        tmpl.insert("host", host);
        tmpl.insert("pid", process::id().to_string());
        tmpl.insert("user", user::get_username().unwrap_or_default());

        tmpl.insert("path", envm.get_self());
        tmpl.insert("path-pretty", envm.contract_home());
        tmpl.insert("path-folder", envm.pretty_dir());
        tmpl.insert("path-short", envm.condensed_path());

        if git_info.in_repo {
            println!("{git_info:#?}");

            tmpl.insert("git.in-repo", true.to_string());
            tmpl.insert("git.branch", git_info.branch_name);
            tmpl.insert("git.ahead", git_info.ahead);
            tmpl.insert("git.behind", git_info.behind);
            tmpl.insert("git.branch.status", git_info.branch_status);
            tmpl.insert("git.stash.count", git_info.stash_count);

            tmpl.insert("git.working.display", git_info.working.status_string);
            tmpl.insert("git.working.unmerged", git_info.working.unmerged);
            tmpl.insert("git.working.deleted", git_info.working.deleted);
            tmpl.insert("git.working.added", git_info.working.added);
            tmpl.insert("git.working.modified", git_info.working.modified);
            tmpl.insert("git.working.untracked", git_info.working.untracked);
            tmpl.insert("git.working.changed", git_info.working.changed.to_string());

            tmpl.insert("git.staging.display", git_info.staging.status_string);
            tmpl.insert("git.staging.unmerged", git_info.staging.unmerged);
            tmpl.insert("git.staging.deleted", git_info.staging.deleted);
            tmpl.insert("git.staging.added", git_info.staging.added);
            tmpl.insert("git.staging.modified", git_info.staging.modified);
            tmpl.insert("git.staging.untracked", git_info.staging.untracked);
            tmpl.insert("git.staging.changed", git_info.staging.changed.to_string());
        }

        tmpl.insert(
            "prompt",
            match unsafe { libc::getuid() } {
                0 => "#",
                _ => "%",
            }
            .to_string(),
        );

        Ok(tmpl.render())
    }

    async fn execute_command(&mut self, line: &String) -> ExitCode {
        let mut exit_code = ExitCode::SUCCESS;
        let commands = TishCommand::parse(line);

        for cmd in commands {
            let result = cmd.execute(self).await;

            let err = match result {
                Ok(_) => continue,
                Err(e) => e,
            };

            if err.to_string().contains("__tish_exit") {
                continue;
            }

            let error_msg = match err.downcast_ref::<std::io::Error>() {
                Some(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                    format!("tish: command not found: {}", cmd.program)
                }
                Some(io_err) => format!("{}: {}", cmd.program, io_err),
                _ => match err.downcast_ref::<String>() {
                    Some(str_err) => str_err.to_string(),
                    None => format!("{}: {err}\n", cmd.program),
                },
            };

            eprintln!("{error_msg}");
            exit_code = ExitCode::FAILURE;
        }

        return exit_code;
    }

    pub async fn run(&mut self) -> Result<ExitCode> {
        let mut status = ExitCode::SUCCESS;

        if let Some(line) = self.args.command.to_owned() {
            if let Err(_) = self.lua.eval(&line) {
                status = self.execute_command(&line).await;
            }
        }

        if self.args.headless {
            let raw_code = unsafe { std::mem::transmute::<ExitCode, u8>(status) };
            process::exit(raw_code as i32);
        }

        loop {
            let prompt = self.format_prompt()?;

            tokio::select! {
                readline = self.readline.async_readline(&prompt) => {
                    match readline {
                        Ok(line) => {
                            if let Err(_) = self.lua.eval(&line) {
                                self.execute_command(&line).await;
                            }
                        }
                        Err(ReadlineError::Interrupted) => {
                            self.readline.clear_buffer();
                            continue;
                        },
                        Err(ReadlineError::Eof) => break,
                        Err(_) => break,
                    }
                }
            }
        }

        Ok(std::process::ExitCode::SUCCESS)
    }
}
