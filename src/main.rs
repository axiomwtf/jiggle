mod build_scripts;
mod bundler;
mod cli;
mod compiler;
mod deps;
mod error;
mod ffi;
mod manifest;
mod parser;
mod vfs;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();
    if let Err(e) = compiler::run(args) {
        eprintln!("jglc: {}", e);
        std::process::exit(1);
    }
}
