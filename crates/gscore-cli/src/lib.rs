//! gscore-cli crate - Command-line interface for GuitarScore
//!
//! This crate provides headless tools for working with Guitar Pro files
//! without requiring the GUI application.

use std::io::{self, Write};

use gscore_io::{import_guitar_pro_file, IoError};
use gscore_model::Song;


/// Display song information from a parsed song
pub fn display_song_info(song: &Song) {
    println!("Song Information:");
    println!("  Title: {}", song.title);
    println!("  Artist: {}", song.artist);
    println!("  Tempo: {} BPM", song.get_tempo());
    println!("  Number of tracks: {}", song.track_count());

    for (i, track) in song.tracks().iter().enumerate() {
        println!("  Track {}: {}", i + 1, track.name);
        println!("    Number of measures: {}", track.measures.len());
        
        if !track.measures.is_empty() {
            let first_measure = &track.measures[0];
            println!("    First measure voices: {}", first_measure.voices.len());
        }
    }
}

/// Run the CLI with provided arguments
pub fn run_cli(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 2 {
        eprintln!("Usage: gscore-cli info <file.gp>");
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "info" => {
            if args.len() < 3 {
                eprintln!("Usage: gscore-cli info <file.gp>");
                std::process::exit(1);
            }
            let file_path = &args[2];
            println!("Displaying info for: {}", file_path);

            // Import the file using our new gscore-io functionality
            match import_guitar_pro_file(std::path::Path::new(file_path)) {
                Ok(song) => {
                    display_song_info(&song);
                }
                Err(e) => {
                    eprintln!("Error importing file: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Available commands: info");
            std::process::exit(1);
        }
    }

    Ok(())
}
