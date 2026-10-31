# Known Gaps in Functionality

This document lists known missing functionality that needs to be implemented.

## Current Gaps

1. **Command System**
   - Base command structure not yet defined
   - No keymap system in place
   
2. **Session Management**
   - Recovery journal not yet implemented
   - Crash recovery mechanism missing
   
3. **Core Crates**
   - `gscore-model` crate missing
   - `gscore-io` not implemented
   - Layout engine missing
   - Playback system not developed
   
4. **UI Layer**
   - No GUI application yet
   - Basic editor features infeasible
   
5. **File Handling**
   - No GP file format support in code
   - CLI tools not implemented

## Priority Order

1. Core model and I/O (M1/M2)
2. Layout engine (M3)
3. UI layer with editing features (M4)
4. Session management with recovery (M5)
5. Playback capabilities (M7/M8)

## Testing Requirements

- Fuzzing tests for file formats
- Panic-injection testing
- Regression tests for stability
- Layout golden diff tests