//! `guitarscore-plugin-host`: runs one CLAP plugin for GuitarScore, apart from the app, so a
//! plugin that crashes or hangs takes only this process down (M24, design note
//! `docs/plugin-hosting.md`). Started by the app, which talks to it on stdin and stdout;
//! not meant to be run by hand. (The app itself serves the same with `--plugin-host`.)

use std::io::{BufReader, BufWriter};

fn main() {
    let input = BufReader::new(std::io::stdin());
    let output = BufWriter::new(std::io::stdout());
    let code = gscore_plugin::serve(input, output);
    std::process::exit(code);
}
