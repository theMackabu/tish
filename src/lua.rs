mod dump;

use crate::prelude::*;
use mlua::prelude::*;
use parking_lot::RwLock;
use sysinfo::System;

use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default)]
pub struct ShellConfig {
    pub prompt: String,
    pub lua_path: Option<String>,
    pub lua_cpath: Option<String>,
    pub history_size: usize,
    pub auto_cd: bool,
    pub builtin_ls: bool,
    pub show_hidden: bool,
}

pub struct LuaState {
    lua: Lua,
    config: Arc<RwLock<ShellConfig>>,
}

impl LuaState {
    pub fn new() -> anyhow::Result<Self> {
        let lua = Lua::new();

        let config = Arc::new(RwLock::new(ShellConfig {
            prompt: "{t.user}@{t.host} {t.cwd} {t.prompt} ".to_string(),
            lua_path: None,
            lua_cpath: None,
            history_size: 1000,
            auto_cd: true,
            builtin_ls: false,
            show_hidden: false,
        }));

        let state = Self { lua, config };
        state.setup_runtime()?;
        Ok(state)
    }

    pub fn setup_runtime(&self) -> anyhow::Result<std::process::ExitCode> {
        let globals = self.lua.globals();

        register_functions!(
            globals,
            "dump" => dump::create_function(&self.lua)?,
            "execute" => sys!(execute => self.lua)?,
            "getenv" => sys!(getenv => self.lua)?,
            "setenv" => sys!(setenv => self.lua)?,
            "alias" => sys!(set_alias => self.lua)?,
            "getalias" => sys!(get_alias => self.lua)?,
            "path_join" => sys!(path_join => self.lua)?,
            "pwd" => sys!(pwd => self.lua)?,
            "ls" => sys!(ls => self.lua)?,
            "mkdir" => sys!(mkdir => self.lua)?,
            "rm" => sys!(rm => self.lua)?,
            "realpath" => sys!(realpath => self.lua)?,
            "dirname" => sys!(dirname => self.lua)?,
            "ps" => sys!(ps => self.lua)?,
            "kill" => sys!(kill => self.lua)?,
            "sysinfo" => sys!(sysinfo => self.lua)?,
            "timestamp" => sys!(timestamp => self.lua)?,

            "set_prompt" => config!(self.lua, self.config, set_prompt, prompt, String)?,
            "set_lua_path" => config!(self.lua, self.config, set_lua_path, lua_path, String, Some)?,
            "set_lua_cpath" => config!(self.lua, self.config, set_lua_cpath, lua_cpath, String, Some)?,
            "set_history_size" => config!(self.lua, self.config, set_history_size, history_size, usize)?,
            "set_auto_cd" => config!(self.lua, self.config, set_auto_cd, auto_cd, bool)?,
            "set_builtin_ls" => config!(self.lua, self.config, set_builtin_ls, builtin_ls, bool)?,
            "set_show_hidden" => config!(self.lua, self.config, set_show_hidden, show_hidden, bool)?
        );

        Ok(std::process::ExitCode::SUCCESS)
    }

    pub fn eval(&self, code: &str) -> anyhow::Result<std::process::ExitCode> {
        self.lua.load(code).exec()?;
        Ok(std::process::ExitCode::SUCCESS)
    }

    pub fn eval_file(&self, path: &std::path::Path) -> anyhow::Result<std::process::ExitCode> {
        let mut code = std::fs::read_to_string(path)?;
        if code.starts_with("#!") {
            code = code.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(code);
        }
        self.eval(&code)
    }

    pub fn get_config(&self) -> Arc<RwLock<ShellConfig>> { self.config.clone() }
}
