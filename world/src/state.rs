#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkState {
    Requested,
    Generating,
    Meshing,
    Resident,
    Evicting,
}