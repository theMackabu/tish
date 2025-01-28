#[macro_export]
macro_rules! lazy_lock {
    ($(#[$attr:meta])* unsafe static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_lock_internal!($(#[$attr])* () static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub unsafe static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_lock_internal!($(#[$attr])* (pub) static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub unsafe ($($vis:tt)+) static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_lock_internal!($(#[$attr])* (pub ($($vis)+)) static $N : $T = $e; $($t)*);
    };
    () => ()
}

#[macro_export]
macro_rules! __lazy_lock_internal {
    ($(#[$attr:meta])* ($($vis:tt)*) static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $(#[$attr])*
        $($vis)* static $N: std::sync::LazyLock<$T> = std::sync::LazyLock::new(|| $e);
        $crate::lazy_lock!($($t)*);
    };
    () => ()
}

#[macro_export]
macro_rules! register_functions {
    ($globals:expr, $($name:expr => $func:expr),* $(,)?) => {
        $($globals.set($name, $func)?;)*
    };
}

#[macro_export]
macro_rules! config {
    ($lua:expr, $config:expr, $name:ident, $field:ident, $type:ty) => {{
        let config_clone = $config.clone();
        $lua.create_function(move |_, value: $type| {
            config_clone.write().$field = value;
            Ok(())
        })
    }};
    ($lua:expr, $config:expr, $name:ident, $field:ident, $type:ty, Some) => {{
        let config_clone = $config.clone();
        $lua.create_function(move |_, value: $type| {
            config_clone.write().$field = Some(value);
            Ok(())
        })
    }};
}

#[macro_export]
macro_rules! sys {
    (execute => $lua:expr) => {{
        $lua.create_function(|_, command: String| {
            let output = Command::new("tish").arg("-H").arg("-c").arg(&command).output().map_err(LuaError::external)?;
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        })
    }};
    (getenv => $lua:expr) => {{
        $lua.create_function(|_, var: String| Ok(std::env::var(var).unwrap_or_default()))
    }};
    (setenv => $lua:expr) => {{
        $lua.create_function(|_, (var, value): (String, String)| {
            std::env::set_var(var, value);
            Ok(())
        })
    }};
    (set_alias => $lua:expr) => {{
        $lua.create_function(|_, (var, value): (String, String)| {
            let mut alias = crate::ALIASES.lock().expect("Able to lock aliases");
            alias.insert(var, value);
            Ok(drop(alias))
        })
    }};
    (get_alias => $lua:expr) => {{
        $lua.create_function(|_, var: String| {
            let alias = crate::ALIASES.lock().expect("Able to lock aliases");
            Ok(alias.get(&var).map(|v| v.to_string()).unwrap_or_default())
        })
    }};
    (path_join => $lua:expr) => {{
        $lua.create_function(|_, parts: Vec<String>| Ok(std::path::Path::new(&parts[0]).join(&parts[1..].join("/")).to_string_lossy().into_owned()))
    }};
    (ls => $lua:expr) => {{
        $lua.create_function(|_, path: Option<String>| {
            let path = path.unwrap_or_else(|| ".".to_string());
            let entries = fs::read_dir(path)?;
            let mut files = Vec::new();
            for entry in entries {
                if let Ok(entry) = entry {
                    files.push(entry.path().to_string_lossy().into_owned());
                }
            }
            Ok(files)
        })
    }};
    (mkdir => $lua:expr) => {{
        $lua.create_function(|_, (path, _): (String, Option<bool>)| {
            fs::create_dir_all(&path)?;
            Ok(())
        })
    }};
    (rm => $lua:expr) => {{
        $lua.create_function(|_, (path, recursive): (String, Option<bool>)| {
            if recursive.unwrap_or(false) {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
            Ok(())
        })
    }};
    (ps => $lua:expr) => {{
        $lua.create_function(|lua, ()| {
            let mut sys = System::new_all();
            sys.refresh_all();

            let processes = sys.processes();
            let process_table = lua.create_table()?;

            for (i, (pid, process)) in processes.iter().enumerate() {
                let process_info = lua.create_table()?;
                process_info.set("pid", pid.as_u32() as i64)?;
                process_info.set("name", process.name())?;
                process_info.set("cpu_usage", process.cpu_usage() as f64)?;
                process_info.set("memory", process.memory() as f64)?;

                process_table.set(i + 1, process_info)?;
            }

            Ok(process_table)
        })
    }};
    (kill => $lua:expr) => {{
        $lua.create_function(|_, pid: i32| {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                kill(Pid::from_raw(pid), Signal::SIGTERM).map_err(LuaError::external)?;
            }
            #[cfg(windows)]
            {
                let output = Command::new("taskkill").arg("/PID").arg(&pid.to_string()).output()?;
                if !output.status.success() {
                    return Err(LuaError::external("Failed to kill process"));
                }
            }
            Ok(())
        })
    }};
    (sysinfo => $lua:expr) => {{
        $lua.create_function(|lua, ()| {
            let mut sys = System::new_all();
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
    }};
    (timestamp => $lua:expr) => {{
        $lua.create_function(|_, ()| {
            let start = SystemTime::now();
            let since_epoch = start.duration_since(UNIX_EPOCH).map_err(LuaError::external)?;
            Ok(since_epoch.as_secs_f64())
        })
    }};
    (realpath => $lua:expr) => {{
        $lua.create_function(|_, path: Option<String>| {
            let canonical = fs::canonicalize(path.unwrap_or_else(|| ".".to_string()))?;
            Ok(canonical.to_string_lossy().into_owned())
        })
    }};
    (dirname => $lua:expr) => {{
        $lua.create_function(|_, path: Option<String>| {
            let path = PathBuf::from(path.unwrap_or_else(|| ".".to_string()));
            Ok(path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".to_string()))
        })
    }};
    (pwd => $lua:expr) => {{
        $lua.create_function(|_, ()| {
            let path = env::current_dir()?;
            Ok(path)
        })
    }};
}
