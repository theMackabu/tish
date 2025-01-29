mod args;
mod cmd;
mod command;
mod jobs;
mod lua;
mod macros;
mod models;
mod os;
mod readline;
mod shell;
mod template;
mod tty;

use anyhow::Result;
use args::{Parser, TishArgs};
use jobs::JobManager;
use shell::TishShell;

use std::{
    collections::HashMap,
    process::ExitCode,
    sync::{Arc, Mutex},
};

type AliasMap = HashMap<String, String>;

lazy_lock! {
    pub unsafe static JOBS: Arc<Mutex<JobManager>> = Arc::new(Mutex::new(JobManager::new()));
    pub unsafe static ALIASES: Arc<Mutex<AliasMap>> = Arc::new(Mutex::new(AliasMap::new()));
}

pub mod prelude {
    pub use super::{argument, config, dotfile, lazy_lock, register_functions, sys};
    pub use anyhow::anyhow;
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let args = TishArgs::parse();
    let mut shell = TishShell::new(args).await?;

    shell.run().await?;
    Ok(ExitCode::SUCCESS)
}
