# Corrupted Test Files

This directory contains deliberately corrupted Guitar Pro files for testing error handling and robustness.

## Purpose

These files are used to ensure that GuitarScore can gracefully handle malformed input without panicking or losing data.

## File Naming Convention

Files should be named with descriptive names indicating the nature of corruption:

- `invalid_header.gp` - Invalid file header
- `malformed_measure.gp` - Malformed measure structure
- `empty_file.gp` - Empty file
- `truncated.gp` - Truncated file data

## Test Coverage

All importers must be tested with these files to ensure robust error handling.
