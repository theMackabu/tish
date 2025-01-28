pub use super::lua::LuaState;

use crate::{
    env::EnvManager,
    models::{Command, InternalCommand},
    shell::TishShell,
};

use anyhow::{anyhow, Result};
use tokio::task;

use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

pub struct TishCommand {
    pub program: String,
    args: Vec<String>,
    background: bool,
}

impl TishCommand {
    pub fn parse(input: &str) -> Vec<Self> {
        let parse = |part| {
            let expanded = EnvManager::new(part).expand();
            let mut tokens = Self::tokenize(&expanded);

            if tokens.is_empty() {
                return None;
            }

            let background = tokens.last().map_or(false, |last| last == "&");
            if background {
                tokens.pop();
            }

            if tokens.is_empty() {
                return None;
            }

            Some(Self {
                program: tokens[0].to_string(),
                args: tokens[1..].iter().map(|s| s.to_string()).collect(),
                background,
            })
        };

        input.split("&&").filter_map(parse).collect()
    }

    pub async fn execute(&self, shell: &TishShell) -> Result<ExitCode> {
        let command = Command::from_str(&self.program, &self.args);
        let internal_command = InternalCommand::from_str(&self.program, &self.args);

        if self.program.as_str() == "tish" && self.args.len() != 0 {
            let result = match internal_command {
                InternalCommand::Jobs => crate::JOBS.lock().expect("Able to lock jobs").list_jobs()?,
                InternalCommand::Help => Self::handle_builtin_help()?,
                InternalCommand::Kill => self.handle_builtin_kill().await?,
                InternalCommand::External => self.execute_external().await?,
                InternalCommand::Script => shell.lua.eval_file(std::path::Path::new(&self.program))?,
                InternalCommand::Pid => {
                    println!("{}", std::process::id());
                    return Ok(ExitCode::SUCCESS);
                }
            };

            return Ok(result);
        }

        let result = match command {
            Command::Cd => self.handle_builtin_cd()?,
            Command::Help => Self::handle_builtin_help()?,
            Command::Exit => std::process::exit(0),
            Command::External => self.execute_external().await?,
            Command::Script => shell.lua.eval_file(std::path::Path::new(&self.program))?,
            Command::Source => shell.lua.eval_file(Path::new(&self.args.get(0).ok_or_else(|| anyhow!("Could not determine source file"))?))?,
        };

        Ok(result)
    }

    async fn execute_external(&self) -> Result<ExitCode> {
        if self.background {
            self.spawn_background_job()?;
            Ok(ExitCode::SUCCESS)
        } else {
            self.spawn_foreground_job().await
        }
    }

    fn spawn_background_job(&self) -> Result<()> {
        let program = self.program.clone();
        let args = self.args.clone();

        task::spawn(async move {
            let mut handle = tokio::process::Command::new(&program);
            handle.args(&args);

            if let Ok(mut jobs) = crate::JOBS.try_lock() {
                if let Err(err) = jobs.add_job(&mut handle) {
                    eprintln!("Failed to add background job: {err}");
                }
            } else {
                eprintln!("Failed to acquire jobs lock for background process");
            }
        });

        Ok(())
    }

    async fn spawn_foreground_job(&self) -> Result<ExitCode> {
        let command = self.resolve_command();
        let mut handles = Vec::new();

        for cmd in &command {
            let child = tokio::process::Command::new(&cmd.program).args(&cmd.args).args(&self.args).spawn()?;
            handles.push(child);
        }

        for mut child in handles {
            child.wait().await?;
        }

        return Ok(ExitCode::SUCCESS);
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

    fn tokenize(input: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut chars = input.chars().peekable();
        let mut in_quotes = false;
        let mut quote_char = None;
        let mut escaped = false;

        while let Some(c) = chars.next() {
            if escaped {
                current_token.push(c);
                escaped = false;
                continue;
            }

            match c {
                '\\' => escaped = true,
                '"' | '\'' => {
                    if !in_quotes {
                        in_quotes = true;
                        quote_char = Some(c);
                    } else if Some(c) == quote_char {
                        in_quotes = false;
                        quote_char = None;
                    } else {
                        current_token.push(c);
                    }
                }
                c if c.is_whitespace() && !in_quotes => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                }
                _ => current_token.push(c),
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        tokens
    }
}
