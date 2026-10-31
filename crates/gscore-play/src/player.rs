//! Player logic for the audio engine
pub trait Player {
    fn play(&mut self) -> Result<(), String>;
    fn pause(&mut self);
    fn stop(&mut self);
}
