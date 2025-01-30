#[derive(Debug)]
pub enum Command {
    Fg,
    Cd,
    Ls,
    Jobs,
    Help,
    Exit,
    Source,
    Script,
    External,
}

#[derive(Debug)]
pub enum InternalCommand {
    Fg,
    Pid,
    Jobs,
    Kill,
    Help,
    Script,
    External,
}

impl Command {
    pub fn from_str(cmd: &str, args: &[String]) -> Command {
        match cmd {
            "fg" => Command::Fg,
            "cd" => Command::Cd,
            "ls" => Command::Ls,
            "exit" => Command::Exit,
            "jobs" => Command::Jobs,
            "source" => Command::Source,
            "help" | "?" => Command::Help,
            "tish" if !args.is_empty() => {
                if args.len() > 2 {
                    Command::Help
                } else {
                    Command::from_str(&args[0], &[])
                }
            }
            path if path.ends_with(".lua") || path.ends_with(".tish") => Command::Script,
            _ => Command::External,
        }
    }
}

impl InternalCommand {
    pub fn from_str(cmd: &str, args: &[String]) -> InternalCommand {
        match cmd {
            "fg" => InternalCommand::Fg,
            "pid" => InternalCommand::Pid,
            "kill" => InternalCommand::Kill,
            "jobs" => InternalCommand::Jobs,
            "help" | "?" => InternalCommand::Help,
            "tish" if !args.is_empty() => {
                if args.len() > 2 {
                    InternalCommand::Help
                } else {
                    InternalCommand::from_str(&args[0], &[])
                }
            }
            path if path.ends_with(".lua") || path.ends_with(".tish") => InternalCommand::Script,
            _ => InternalCommand::External,
        }
    }
}
