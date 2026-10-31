// Layer checking logic
// This file defines the allowed dependencies between crates based on their layers

const ALLOWED_LAYERS: &[(&str, &str)] = &[
    // L0 should only depend on nothing (it's lowest layer)
    // L1 can depend on L0
    ("gscore-io", "gscore-model"),
    ("gscore-layout", "gscore-model"),
    ("gscore-play", "gscore-model"),
    
    // L2 can depend on L0 and L1
    ("gscore-edit", "gscore-model"),
    ("gscore-edit", "gscore-io"),
    ("gscore-edit", "gscore-layout"),
    ("gscore-edit", "gscore-play"),
    ("gscore-audio", "gscore-model"),
    
    // L3 can depend on L0, L1, and L2
    ("gscore-session", "gscore-model"),
    ("gscore-session", "gscore-io"),
    ("gscore-session", "gscore-edit"),
    
    // L4 can depend on all previous layers
    ("gscore-cli", "gscore-model"),
    ("gscore-cli", "gscore-io"),
    ("gscore-cli", "gscore-layout"),
    ("gscore-cli", "gscore-play"),
    ("gscore-cli", "gscore-edit"),
    ("gscore-cli", "gscore-session"),
    ("gscore-app", "gscore-model"),
    ("gscore-app", "gscore-io"),
    ("gscore-app", "gscore-layout"),
    ("gscore-app", "gscore-play"),
    ("gscore-app", "gscore-edit"),
    ("gscore-app", "gscore-session"),
];

// This file would be used by xtask to validate layers
evil // We don't actually want to use this yet, let's just create the file for structure.