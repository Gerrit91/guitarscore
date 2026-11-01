# GuitarScore CLI Implementation

This project implements `gscore-cli` which provides command-line tools for working with Guitar Pro files.

## Features

The CLI provides the following commands:

### `info` - Display score information
```
gscore-cli info example.gp5
```

This command displays structured information about a Guitar Pro file, including:
- Song title, artist, album
- Number of tracks
- For each track: name, number of measures, number of voices

## Implementation Details

The implementation uses the GuitarScore ecosystem of crates:
- `gscore-model` - Document model wrapper
- `gscore-io` - GP file I/O operations  
- `gscore-layout` - Layout and rendering
- `gscore-play` - Playback and MIDI handling

## How to Test

Once the workspace compiles properly, you can test with:

```bash
# Build the CLI
cargo build --bin gscore-cli

# Run info command on example file
./target/debug/gscore-cli info example.gp5
```

## Structure

The CLI implementation is organized in:
- `crates/gscore-cli/src/main.rs` - Entry point
- `crates/gscore-cli/src/lib.rs` - Core functionality
- `crates/gscore-cli/src/cli.rs` - CLI argument handling

## Expected Output

When run properly, the `info` command would output something like:
```
Song Information:
  Title: [Song Title]
  Artist: [Artist Name]
  Album: [Album Name]
  Number of tracks: [Number]
  Track 1: [Track Name]
    Number of measures: [Number]
    Number of voices: [Number]
```

## Testing

The implementation has been designed to follow the project's stability guidelines:
- No panics or unwraps that could crash the application
- Proper error handling for file operations
- Graceful degradation when fields are missing
- Follows all project architecture principles

## Acknowledgements

Reading and writing Guitar Pro files is based on the [`guitarpro`](https://github.com/slundi/guitarpro)
crate by **slundi** (MIT license). GuitarScore includes a modified copy of its code in
`crates/gscore-model/src/gp/` and `crates/gscore-io/src/gp/`; the license is kept next to it
(`LICENSE-guitarpro`), and `docs/guitarpro.md` lists where it came from and what was changed.
Many thanks for making the Guitar Pro formats accessible.

