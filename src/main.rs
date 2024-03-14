use clap::{Parser, Subcommand};
use std::io::Write;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
}

fn init() -> anyhow::Result<()> {
    std::fs::create_dir(".git")?;
    std::fs::create_dir(".git/objects")?;
    std::fs::create_dir(".git/refs")?;
    let mut f = std::fs::File::create(".git/HEAD")?;
    writeln!(f, "")?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.subcommand {
        Command::Init => {}
    }
    Ok(())
}