mod parser;
mod manifest;
mod vfs;
mod deps;
mod bundler;
mod build_scripts;
mod ffi;
mod compiler;
mod cli;
mod error;

use clap::Parser;

fn main() {
    let args = cli::Cli::parse();
    if let Err(e) = compiler::run(args) {
        eprintln!("jglc: {}", e);
        std::process::exit(1);
    }
}
