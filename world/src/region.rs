use crate::chunk::Chunk;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};
use voxel_core::ChunkCoord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionChunkData {
    pub coord: ChunkCoord,
    pub chunk: Chunk,
}

#[derive(Debug, Clone)]
pub struct RegionStore {
    root: PathBuf,
}

impl RegionStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load_chunk(&self, coord: ChunkCoord) -> io::Result<Option<Chunk>> {
        let path = self.chunk_path(coord);
        if !path.exists() {
            return Ok(None);
        }

        let compressed = fs::read(path)?;
        let bytes = decompress_size_prepended(&compressed)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let data: RegionChunkData = bincode::deserialize(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        Ok(Some(data.chunk))
    }

    pub fn save_chunk(&self, chunk: &Chunk) -> io::Result<()> {
        let path = self.chunk_path(chunk.coord);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = RegionChunkData {
            coord: chunk.coord,
            chunk: chunk.clone(),
        };
        let bytes = bincode::serialize(&payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let compressed = compress_prepend_size(&bytes);
        fs::write(path, compressed)?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn chunk_path(&self, coord: ChunkCoord) -> PathBuf {
        let region = coord.to_region();
        let region_dir = self.root.join(format!("r.{}.{}", region.x, region.z));
        region_dir.join(format!("c.{}.{}.bin", coord.x, coord.z))
    }
}
