//! Generate the `clearhead.1` man page from the clap `Cli` definition.
//!
//! Usage:
//!   cargo run --bin gen-man                    # writes to man/clearhead.1
//!   cargo run --bin gen-man -- /custom/path    # writes to a custom directory

use clap::CommandFactory;
use clap_mangen::Man;
use clearhead_cli::argparser::Cli;
use std::{fs, path::PathBuf};

fn main() {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("man"));

    fs::create_dir_all(&out_dir).expect("failed to create output directory");

    let cmd = Cli::command().name("clearhead");
    let man = Man::new(cmd);

    let mut buf = Vec::new();
    man.render(&mut buf).expect("failed to render man page");

    let out_path = out_dir.join("clearhead.1");
    fs::write(&out_path, buf).expect("failed to write man page");

    println!("Man page written to {}", out_path.display());
}
