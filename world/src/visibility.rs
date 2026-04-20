use voxel_core::ChunkCoord;

pub fn nearest_visible_coords(center: ChunkCoord, radius: i32, max_chunks: usize) -> Vec<ChunkCoord> {
    let distance_sq_limit = radius * radius;
    let mut coords = Vec::new();

    for z in -radius..=radius {
        for x in -radius..=radius {
            let coord = ChunkCoord::new(center.x + x, center.z + z);
            let distance_sq = center.distance_squared(coord);
            if distance_sq <= distance_sq_limit {
                coords.push((distance_sq, coord));
            }
        }
    }

    coords.sort_unstable_by_key(|&(distance_sq, coord)| (distance_sq, coord.x, coord.z));
    coords
        .into_iter()
        .take(max_chunks)
        .map(|(_, coord)| coord)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxel_core::ChunkCoord;

    #[test]
    fn visible_chunk_requests_are_capped() {
        let coords = nearest_visible_coords(ChunkCoord::new(0, 0), 3, 16);
        assert_eq!(coords.len(), 16);
        assert_eq!(coords[0], ChunkCoord::new(0, 0));
    }
}