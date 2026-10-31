// Main entry point for xtask commands
use std::process::exit;

fn main() {
    println!("GuitarScore xtask commands:");
    println!();
    println!("Available commands:");
    println!("  layers     - Check layer dependencies");
    println!("  corpus     - Manage corpus tests");
    println!("  ci         - Run CI pipeline checks");
    println!();
    println!("Run with --help for more information about each command.");
    exit(1);
}
