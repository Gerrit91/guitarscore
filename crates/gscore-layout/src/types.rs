//! Layout types and utility structures

/// Position in 2D space for draw operations
#[derive(Debug, Clone)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub fn new(x: f32, y: f32) -> Self {
        Position { x, y }
    }
}

/// Size in 2D space
#[derive(Debug, Clone)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}
