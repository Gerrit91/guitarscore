//! `guitarscore-plugin-host`: runs one CLAP plugin for GuitarScore, apart from the app, so a
//! plugin that crashes or hangs takes only this process down (M24, design note
//! `docs/plugin-hosting.md`). Started by the app, which talks to it on stdin and stdout;
//! not meant to be run by hand.

use std::io::{BufReader, BufWriter};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());
    let code = gscore_plugin::serve(&mut input, &mut output);
    std::process::exit(code);
}
