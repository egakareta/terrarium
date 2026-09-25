use std::{path::PathBuf, process::ExitCode};

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "terrarium")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Set up a Terrarium project.
    Setup {
        /// Directory in which to install the project files.
        #[arg(value_name = "DIRECTORY", default_value = ".")]
        directory: PathBuf,
    },
}

fn main() -> ExitCode {
    let Some(Command::Setup { directory }) = Cli::parse().command else {
        Cli::command().print_help().expect("failed to print help");
        return ExitCode::SUCCESS;
    };

    #[cfg(feature = "javascript")]
    match terrarium::setup_script_project(&directory) {
        Ok(()) => {
            println!("terrarium installed into {}", directory.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to set up {}: {error}", directory.display());
            ExitCode::from(1)
        }
    }

    #[cfg(not(feature = "javascript"))]
    {
        let _ = directory;
        eprintln!("the `setup` command requires the `javascript` feature");
        ExitCode::from(1)
    }
}
