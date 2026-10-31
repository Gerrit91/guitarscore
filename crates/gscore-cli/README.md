# gscore-cli

Command-line interface for GuitarScore, providing headless tools for working with Guitar Pro files.

## Features

- `info` - Display information about a Guitar Pro file
- `dump` - Dump the contents of a score to ASCII tablature
- `render` - Render a score to SVG or PNG
- `convert` - Convert between different Guitar Pro formats
- `roundtrip-check` - Verify round-trip conversion consistency

## Usage

```bash
# Display information about a score
gscore-cli info example.gp5

# Dump score to ASCII tablature
gscore-cli dump example.gp5

# Convert to different format
gscore-cli convert example.gp5 output.gp7
```

## Implementation

This CLI tool is designed to work with the GuitarScore ecosystem of crates:

- `gscore-model` - Document model
- `gscore-io` - GP file I/O operations
- `gscore-layout` - Layout and rendering
- `gscore-play` - Playback and MIDI handling

The CLI provides headless access to the core functionality without requiring a GUI.