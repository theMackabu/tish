#[macro_export]
macro_rules! lazy_lock {
    ($(#[$attr:meta])* static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_lock_internal!($(#[$attr])* () static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        $crate::__lazy_lock_internal!($(#[$attr])* (pub) static $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub ($($vis:tt)+) static $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
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
macro_rules! dotfile {
    ($path:expr => $dot:expr, $command:expr) => {
        if let Some(home) = &$path {
            let file = home.join($dot);
            if file.exists() {
                $command(&file)?;
            }
        }
        Ok(ExitCode::SUCCESS)
    };

    (not, $path:expr => $dot:expr, $command:expr) => {
        if let Some(home) = &$path {
            let file = home.join($dot);
            if !file.exists() {
                $command(&file);
            }
        }
        Ok(ExitCode::SUCCESS)
    };
}

#[macro_export]
macro_rules! argument {
    (
        args: $args:expr,
        options: { $( $opt:ident => $set:expr ),* },
        command: $command:expr,
        on_invalid: $on_invalid:expr
    ) => {
        for arg in $args {
            if arg.starts_with('-') && arg.len() > 1 {
                for c in arg[1..].chars() {
                    match c {
                        $(ch if ch == stringify!($opt).chars().next().unwrap() => $set, )*
                        _ => {
                            $on_invalid(c);
                            return Ok(ExitCode::SUCCESS);
                        }
                    }
                }
            } else { $command(&arg) }
        }
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
macro_rules! define {
    ($lua:expr, $globals:expr, $name:expr, $func:expr) => {
        $globals.set($name, $lua.create_function($func)?)?
    };
}
