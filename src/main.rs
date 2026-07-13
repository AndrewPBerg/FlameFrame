mod agent;
mod cli;
mod context;
mod diagnostics;
mod ffmpeg;
mod manual;
mod pipeline;
mod uninstall;
mod upgrade;
mod workspace;
mod ytdlp;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Man => manual::print(),
        Command::Doctor(args) => diagnostics::run_doctor(args.json),
        Command::Upgrade(args) => upgrade::run(&args),
        Command::Uninstall => uninstall::run(),
        Command::Agent { command } => match command {
            cli::AgentCommand::Install(args) => agent::install(&args),
        },
        Command::Process(args) => pipeline::process(&args),
        Command::Ingest(args) => pipeline::ingest(&args),
        Command::Download(args) => pipeline::download(&args),
        Command::Inspect(args) => pipeline::inspect(&args),
        Command::Split(args) => pipeline::split(&args),
        Command::Context(args) => pipeline::context(&args),
        Command::Verify(args) => pipeline::verify(&args),
        Command::Zoom(args) => pipeline::zoom(&args),
    }
}
