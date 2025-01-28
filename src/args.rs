pub use clap::Parser;

#[derive(Parser, Clone)]
#[command(author, version)]
pub struct TishArgs {
    #[command()]
    pub arguments: Option<String>,

    /// Execute command and exit
    #[arg(short = 'c', long)]
    pub command: Option<String>,

    /// Don't load environment
    #[arg(short = 'n', long = "no-env")]
    pub no_env: bool,

    /// Run in headless mode
    #[arg(short = 'H', long)]
    pub headless: bool,

    /// Login shell (loads .tish_profile)
    #[arg(short = 'L')]
    pub login: bool,
}
