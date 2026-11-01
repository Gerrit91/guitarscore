//! Playback sequencing: turns the model into timed MIDI events, allocates MIDI channels, and
//! defines the `Player` trait that output backends implement. No device crates here.
//!
//! Not implemented yet (milestone M7; AGENTS.md section 5, decisions 8-10, and section 8).

#![warn(clippy::arithmetic_side_effects)]
