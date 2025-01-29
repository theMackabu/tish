pub mod highlight;
pub mod tokenizer;

use crate::{
    args::TishArgs,
    command::{LuaState, TishCommand},
    env::EnvManager,
    os::user,
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
use tokio::signal::unix::{signal, SignalKind};

pub struct TishShell {
    pub args: TishArgs,
    pub lua: LuaState,
    home: Option<PathBuf>,
}

impl TishShell {
    pub async fn new(args: TishArgs) -> Result<Self> {
        let mut shell = Self {
            args: args.to_owned(),
            lua: LuaState::new()?,
            home: dirs::home_dir(),
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

    fn format_prompt(&self) -> String {
        let config = self.lua.get_config();
        let template_str = config.read().prompt.to_owned();
        let mut template = Template::new(&template_str);

        fn determine_prompt_symbol() -> Result<String, Box<dyn std::error::Error>> {
            let uid = unsafe { libc::getuid() };
            if uid == 0 {
                Ok("#".to_string())
            } else {
                Ok("%".to_string())
            }
        }

        // TODO: Rewrite this function to be much cleaner
        let pid = process::id().to_string();
        let host = hostname::get().unwrap().to_string_lossy().to_string();
        let path = env::current_dir().unwrap().to_string_lossy().to_string();
        let current_dir = env::current_dir().unwrap().to_string_lossy().to_string();

        let display_dir = if current_dir == "/" {
            "/".to_string()
        } else {
            env::current_dir().unwrap().file_name().unwrap_or_default().to_string_lossy().to_string()
        };

        template.insert("pid", pid);
        template.insert("user", user::get_username().unwrap_or_default());
        template.insert("host", host);

        // TODO: Improve ENVManager to be dynamic loaded, no need for new classes
        template.insert("path", EnvManager::new(&display_dir).pretty_dir());
        template.insert("cwd", EnvManager::new(&path).contract_home());

        if let Ok(symbol) = determine_prompt_symbol() {
            template.insert("prompt", symbol.to_string());
        }

        return template.render();
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

            let error_msg = match err.downcast_ref::<std::io::Error>() {
                Some(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => format!("tish: command not found: {}", cmd.program),
                Some(io_err) => format!("{}: {}", cmd.program, io_err),
                _ => match err.downcast_ref::<String>() {
                    Some(str_err) => str_err.to_string(),
                    None => format!("{}: {err}", cmd.program),
                },
            };

            eprintln!("{error_msg}");
            exit_code = ExitCode::FAILURE;
        }

        return exit_code;
    }

    pub async fn run(&mut self) -> Result<ExitCode> {
        let mut status = ExitCode::SUCCESS;
        let mut rl = AsyncLineReader::new()?;
        let mut sigint = signal(SignalKind::interrupt())?;

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
            let prompt = self.format_prompt();

            tokio::select! {
                readline = rl.async_readline(&prompt) => {
                    match readline {
                        Ok(line) => {
                            rl.add_history_entry(&line).await?;
                            if let Err(_) = self.lua.eval(&line) {
                                self.execute_command(&line).await;
                            }
                        }
                        Err(ReadlineError::Interrupted) => {
                            rl.clear_buffer();
                            continue;
                        },
                        Err(ReadlineError::Eof) => break,
                        Err(_) => break,
                    }
                }
                _ = sigint.recv() => {
                    rl.clear_buffer();
                    continue;
                },
            }
        }

        Ok(std::process::ExitCode::SUCCESS)
    }
}
