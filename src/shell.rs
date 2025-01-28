use crate::{
    args::TishArgs,
    command::{LuaState, TishCommand},
    env::EnvManager,
    readline::AsyncLineReader,
    template::Template,
};

use std::{
    env,
    process::{self, ExitCode},
};

use rustyline::error::ReadlineError;
use tokio::signal::unix::{signal, SignalKind};

pub struct TishShell {
    pub args: TishArgs,
    pub lua: LuaState,
}

impl TishShell {
    pub async fn new(args: TishArgs) -> anyhow::Result<Self> {
        let mut shell = Self {
            args: args.to_owned(),
            lua: LuaState::new()?,
        };

        if !args.no_env {
            shell.load_config()?;
        }

        if args.login {
            shell.load_profile()?;
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

    fn load_config(&self) -> anyhow::Result<ExitCode> {
        if let Some(home) = dirs::home_dir() {
            let config = home.join(".tishrc");
            if config.exists() {
                self.lua.eval_file(&config)?;
            }
        }
        Ok(ExitCode::SUCCESS)
    }

    fn load_profile(&self) -> anyhow::Result<ExitCode> {
        if let Some(home) = dirs::home_dir() {
            let profile = home.join(".tish_profile");
            if profile.exists() {
                self.lua.eval_file(&profile)?;
            }
        }
        Ok(ExitCode::SUCCESS)
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

        let pid = process::id().to_string();
        let user = env::var("USER").unwrap_or_default().to_string();
        let host = hostname::get().unwrap().to_string_lossy().to_string();
        let path = env::current_dir().unwrap().to_string_lossy().to_string();

        template.insert("pid", pid);
        template.insert("user", user);
        template.insert("host", host);
        template.insert("path", EnvManager::new(&path).contract_home());

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

    pub async fn run(&mut self) -> anyhow::Result<ExitCode> {
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
