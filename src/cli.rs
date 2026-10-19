use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "jglc")]
#[command(about = "Jiggle Language compiler - compile polyglot .jgl projects into native binaries")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compile a .jgl project into a native binary
    #[command(alias = "build")]
    Compile {
        /// Path to the .jgl project file (default: project.jgl)
        #[arg(short, long, default_value = "project.jgl")]
        project: PathBuf,

        /// Output directory for the bundle and build scripts
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// Only generate the bundle and build scripts, don't compile
        #[arg(long)]
        bundle_only: bool,

        /// Target Windows (produce .exe binary via cross-compilation)
        #[arg(long)]
        target_win: bool,
    },

    /// Validate a .jgl project without compiling
    #[command(alias = "check")]
    Validate {
        /// Path to the .jgl project file
        #[arg(short, long, default_value = "project.jgl")]
        project: PathBuf,
    },

    /// Generate just the build scripts
    #[command(alias = "gen")]
    Generate {
        /// Path to the .jgl project file
        #[arg(short, long, default_value = "project.jgl")]
        project: PathBuf,

        /// Output directory for the build scripts
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// Target Windows (produce .exe binary via cross-compilation)
        #[arg(long)]
        target_win: bool,
    },
}
