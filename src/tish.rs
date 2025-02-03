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
use dashmap::DashSet;
use jobs::JobManager;
use shell::TishShell;

use std::{
    collections::HashMap,
    process::ExitCode,
    sync::{Arc, Mutex},
};

type AliasMap = HashMap<String, String>;

lazy_lock! {
    pub static LUA_FN: Arc<DashSet<String>> = Arc::new(DashSet::new());
    pub static JOBS: Arc<Mutex<JobManager>> = Arc::new(Mutex::new(JobManager::new()));
    pub static ALIASES: Arc<Mutex<AliasMap>> = Arc::new(Mutex::new(AliasMap::new()));
}

pub mod prelude {
    pub use super::{argument, config, define, dotfile, env_set_sync, lazy_lock};
    pub use anyhow::anyhow;
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let args = TishArgs::parse();
    let mut shell = TishShell::new(args).await?;

    env_set_sync! {
        "0" => "tish",
        "LUA_VER" => "5.4",
        "VERSION" => env!("CARGO_PKG_VERSION")
    };

    shell.run().await?;
    Ok(ExitCode::SUCCESS)
}
