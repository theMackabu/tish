mod modules;

use crate::prelude::*;
use libc::pid_t;
use mlua::prelude::*;

use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

struct LuaProcess {
    pid: u32,
}

struct FileWrapper {
    file: File,
}

struct LuaFile;

struct LuaEnv;

struct LuaAlias;

struct LuaSystem;

impl LuaUserData for LuaProcess {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("list", |lua, ()| {
            let mut sys = sysinfo::System::new_all();
            sys.refresh_all();

            let processes = sys.processes();
            let process_table = lua.create_table()?;

            for (i, (pid, process)) in processes.iter().enumerate() {
                let process_info = lua.create_table()?;
                process_info.set("pid", pid.as_u32() as i64)?;
                process_info.set("name", process.name())?;
                process_info.set("memory", process.memory() as f64)?;
                process_info.set("cpu_usage", process.cpu_usage() as f64)?;
                process_table.set(i + 1, process_info)?;
            }

            Ok(process_table)
        });

        methods.add_function("kill", |_, pid: pid_t| {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            Ok(kill(Pid::from_raw(pid), Signal::SIGTERM).map_err(LuaError::external)?)
        });

        methods.add_function("exit", |lua: &Lua, code: Option<i32>| -> LuaResult<()> {
            let code = code.unwrap_or(0);
            lua.set_named_registry_value("__tish_exit_code", code)?;
            let co = lua.create_function(|_, ()| -> LuaResult<()> { Err(LuaError::external("__tish_exit")) })?;
            Ok(co.call(())?)
        });
    }

    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_function_get("cwd", |_, _| Ok(env::current_dir()?));
        fields.add_field_method_get("pid", |_, process| Ok(process.pid));
        fields.add_field_function_get("ppid", |_, _| Ok(nix::unistd::getppid().as_raw()));
        fields.add_field_function_get("euid", |_, _| Ok(nix::unistd::getpgrp().as_raw()));
    }
}

impl LuaUserData for LuaFile {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("read_dir", |_, path: Option<String>| {
            let path = path.unwrap_or_else(|| ".".to_string());
            let entries = fs::read_dir(path)?;
            let mut files = Vec::new();
            for entry in entries {
                if let Ok(entry) = entry {
                    files.push(entry.path().to_string_lossy().into_owned());
                }
            }
            Ok(files)
        });

        methods.add_function("create_file", |_, path: String| {
            let file = File::create(&path)?;
            Ok(FileWrapper { file })
        });

        methods.add_function("open_file", |_, (path, mode): (String, Option<String>)| {
            use std::fs::OpenOptions;
            let mut options = OpenOptions::new();

            match mode.as_deref() {
                Some("w") => options.write(true).truncate(true).create(true),
                Some("a") => options.append(true).create(true),
                Some("r+") => options.read(true).write(true),
                Some("w+") => options.read(true).write(true).truncate(true).create(true),
                Some("a+") => options.read(true).append(true).create(true),
                _ => options.read(true),
            };

            let file = options.open(&path)?;
            Ok(FileWrapper { file })
        });

        methods.add_function("create_dir", |_, path: String| Ok(fs::create_dir(&path)?));
        methods.add_function("create_dir_all", |_, path: String| Ok(fs::create_dir_all(&path)?));
        methods.add_function("remove_file", |_, path: String| Ok(fs::remove_file(&path)?));
        methods.add_function("remove_dir", |_, path: String| Ok(fs::remove_dir(&path)?));
        methods.add_function("remove_dir_all", |_, path: String| Ok(fs::remove_dir_all(&path)?));

        methods.add_function("real_path", |_, path: Option<String>| {
            let canonical = fs::canonicalize(path.unwrap_or_else(|| ".".to_string()))?;
            Ok(canonical.to_string_lossy().into_owned())
        });

        methods.add_function("dir_name", |_, path: Option<String>| {
            let path = PathBuf::from(path.unwrap_or_else(|| ".".to_string()));
            Ok(path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".to_string()))
        });

        methods.add_function("join_path", |_, parts: Vec<String>| {
            let path = Path::new(&parts[0]);
            Ok(path.join(&parts[1..].join("/")).to_string_lossy().into_owned())
        });
    }

    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_function_get("temp_dir", |_, _| Ok(env::temp_dir().to_string_lossy().to_string()));
        fields.add_field_function_get("home_dir", |_, _| Ok(dirs::home_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
        fields.add_field_function_get("config_dir", |_, _| Ok(dirs::config_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
        fields.add_field_function_get("cache_dir", |_, _| Ok(dirs::cache_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
    }
}

impl LuaUserData for FileWrapper {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};

        methods.add_method_mut("flush", |_, this, ()| this.file.flush().map_err(LuaError::external));
        methods.add_method_mut("write", |_, this, data: String| this.file.write_all(data.as_bytes()).map_err(LuaError::external));

        methods.add_method_mut("write_line", |_, this, data: String| {
            this.file.write_all(data.as_bytes()).map_err(LuaError::external)?;
            this.file.write_all(b"\n").map_err(LuaError::external)
        });

        methods.add_method("read_all", |_, this, ()| {
            let mut file = this.file.try_clone().map_err(LuaError::external)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents).map_err(LuaError::external)?;
            Ok(contents)
        });

        methods.add_method("read_line", |_, this, ()| {
            let file = this.file.try_clone().map_err(LuaError::external)?;
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            reader.read_line(&mut line).map_err(LuaError::external)?;
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(line)
        });

        methods.add_method("read", |_, this, bytes: Option<usize>| {
            let mut file = this.file.try_clone().map_err(LuaError::external)?;
            match bytes {
                Some(n) => {
                    let mut buffer = vec![0; n];
                    let bytes_read = file.read(&mut buffer).map_err(LuaError::external)?;
                    buffer.truncate(bytes_read);
                    Ok(String::from_utf8_lossy(&buffer).into_owned())
                }
                None => {
                    let mut contents = String::new();
                    file.read_to_string(&mut contents).map_err(LuaError::external)?;
                    Ok(contents)
                }
            }
        });

        methods.add_method_mut("seek", |_, this, (pos, whence): (i64, Option<String>)| {
            let seek_from = match whence.as_deref() {
                Some("set") => SeekFrom::Start(pos as u64),
                Some("end") => SeekFrom::End(pos),
                _ => SeekFrom::Current(pos),
            };
            this.file.seek(seek_from).map_err(LuaError::external)
        });
    }
}

impl LuaUserData for LuaEnv {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("unset", |_, name: String| Ok(env::remove_var(name)));
        methods.add_meta_method(LuaMetaMethod::Index, |_, _, key: String| Ok(env::var(key).ok()));
        methods.add_meta_method_mut(LuaMetaMethod::NewIndex, |_, _, (key, value): (String, String)| Ok(env_set_sync!(key => value)));
    }
}

impl LuaUserData for LuaAlias {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::Index, |_, _, key: String| {
            let alias = crate::ALIASES.lock().expect("Able to lock aliases");
            Ok(alias.get(&key).map(|v| v.to_string()).unwrap_or_default())
        });

        methods.add_meta_method(LuaMetaMethod::NewIndex, |_, _, (key, value): (String, String)| {
            let mut alias = crate::ALIASES.lock().expect("Able to lock aliases");
            alias.insert(key, value);
            Ok(drop(alias))
        });
    }
}

impl LuaUserData for LuaSystem {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("uptime", |_, ()| Ok(sysinfo::System::uptime()));

        methods.add_function("eval_with_stdout", |_, command: String| {
            let output = Command::new("tish").arg("-H").arg("-c").arg(&command).output().map_err(LuaError::external)?;
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        });

        methods.add_function("timestamp", |_, ()| {
            let start = SystemTime::now();
            let since_epoch = start.duration_since(UNIX_EPOCH).map_err(LuaError::external)?;
            Ok(since_epoch.as_secs_f64())
        });

        methods.add_function("info", |lua, ()| {
            let mut sys = sysinfo::System::new_all();
            sys.refresh_all();

            let info_table = lua.create_table()?;
            info_table.set("total_memory", sys.total_memory() as f64)?;
            info_table.set("used_memory", sys.used_memory() as f64)?;
            info_table.set("total_swap", sys.total_swap() as f64)?;
            info_table.set("used_swap", sys.used_swap() as f64)?;
            info_table.set("cpu_count", sys.cpus().len() as i64)?;
            info_table.set("cpu_usage", sys.global_cpu_usage() as f64)?;

            Ok(info_table)
        })
    }

    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_function_get("hostname", |_, _| Ok(hostname::get()?.to_string_lossy().to_string()));
        fields.add_field_function_get("os_type", |_, _| Ok(env::consts::OS.to_string()));
        fields.add_field_function_get("os_arch", |_, _| Ok(env::consts::ARCH.to_string()));
        fields.add_field_function_get("os_family", |_, _| Ok(env::consts::FAMILY.to_string()));

        fields.add_field_function_get("boot_time", |_, _| {
            Ok(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 - sysinfo::System::uptime() as i64)
        });

        fields.add_field_function_get("max_pid", |_, _| {
            Ok(fs::read_to_string("/proc/sys/kernel/pid_max").ok().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(32768))
        });
    }
}

pub struct LuaState {
    lua: Lua,
    config: Option<LuaRegistryKey>,
}

impl Drop for LuaState {
    fn drop(&mut self) {
        if let Some(key) = self.config.take() {
            let _ = self.lua.remove_registry_value(key);
        }
    }
}

impl LuaState {
    pub fn new() -> anyhow::Result<Self> {
        let lua = Lua::new();
        let cfg_table = lua.create_table()?;

        cfg_table.set("lua_path", LuaNil)?;
        cfg_table.set("lua_cpath", LuaNil)?;
        cfg_table.set("history_size", 500)?;
        cfg_table.set("auto_cd", true)?;
        cfg_table.set("use_tish_ls", false)?;
        cfg_table.set("show_hidden", false)?;
        cfg_table.set("prompt", "{t.user}@{t.host} {t.cwd} {t.prompt} ")?;

        let config = Some(lua.create_registry_value(cfg_table)?);
        let state = Self { lua, config };

        if let Some(ref registry) = state.config {
            state.lua.globals().set("config", registry)?;
        }

        state.setup_runtime()?;
        Ok(state)
    }

    pub fn setup_runtime(&self) -> anyhow::Result<std::process::ExitCode> {
        // TODO: make this use
        // wrapper around "require" that then
        // https://www.lua.org/pil/8.1.html
        // local process = require("process")
        let globals = self.lua.globals();
        let tish = self.lua.create_table()?;
        let process = LuaProcess { pid: std::process::id() };

        globals.set("alias", LuaAlias)?;
        globals.set("fs", LuaFile)?;
        globals.set("env", LuaEnv)?;
        globals.set("sys", LuaSystem)?;

        globals.set("process", process)?;
        globals.set("tish", tish)?;

        define! {
            self.lua, globals, "dump",
            |_, value: LuaValue| Ok(println!("{value:#?}"))
        }

        Ok(ExitCode::SUCCESS)
    }

    pub fn eval(&self, code: &str) -> anyhow::Result<std::process::ExitCode> {
        match self.lua.load(code).exec() {
            Ok(_) => Ok(ExitCode::SUCCESS),
            Err(LuaError::ExternalError(err)) if err.to_string() == "__tish_exit" => {
                let code = self.lua.named_registry_value::<i32>("__tish_exit_code")?;
                Ok(ExitCode::from(code as u8))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn eval_file(&self, path: &std::path::Path) -> anyhow::Result<std::process::ExitCode> {
        let mut code = std::fs::read_to_string(path)?;
        if code.starts_with("#!") {
            code = code.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(code);
        }
        self.eval(&code)
    }

    pub fn get_config_value<T: FromLua>(&self, key: &str) -> anyhow::Result<T> {
        if let Some(ref registry_key) = self.config {
            let config: LuaTable = self.lua.registry_value(registry_key)?;
            Ok(config.get(key)?)
        } else {
            anyhow::bail!("Config not initialized")
        }
    }

    pub fn set_config_value<T: IntoLua>(&self, key: &str, value: T) -> anyhow::Result<()> {
        if let Some(ref registry_key) = self.config {
            let config: LuaTable = self.lua.registry_value(registry_key)?;
            Ok(config.set(key, value)?)
        } else {
            anyhow::bail!("Config not initialized")
        }
    }
}
