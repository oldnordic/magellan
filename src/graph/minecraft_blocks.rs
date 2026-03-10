//! Minecraft-style block storage for GeometricBackend
//!
//! Provides O(1) block-based storage with 4D spatial indexing (x, y, z, t)
//! using Morton encoding for geographic clustering.

use anyhow::Result;
use std::collections::HashMap;

/// 4D Morton encoding for spatial indexing
///
/// Interleaves bits from x, y, z, t coordinates to create a single
/// 64-bit key that preserves spatial locality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Morton4(u64);

impl Morton4 {
    /// Block size in each dimension (16 units)
    pub const BLOCK_SIZE: u32 = 16;

    /// Create Morton4 from 4D coordinates
    ///
    /// Coordinates are divided by BLOCK_SIZE to get block coordinates.
    pub fn from_coords(x: u32, y: u32, z: u32, t: u32) -> Self {
        let bx = x / Self::BLOCK_SIZE;
        let by = y / Self::BLOCK_SIZE;
        let bz = z / Self::BLOCK_SIZE;
        let bt = t / Self::BLOCK_SIZE;
        Self::encode(bx, by, bz, bt)
    }

    /// Encode 4 16-bit values into a 64-bit Morton code
    ///
    /// Interleaves bits: t3z3y3x3 t2z2y2x2 t1z1y1x1 t0z0y0x0
    fn encode(x: u32, y: u32, z: u32, t: u32) -> Self {
        // Each coordinate can use up to 16 bits (64 bits / 4 dimensions)
        // We interleave them bit by bit
        let mut result: u64 = 0;

        for i in 0..16 {
            let bit_mask = 1u32 << i;

            // Extract bit i from each coordinate
            let x_bit = ((x & bit_mask) >> i) as u64;
            let y_bit = ((y & bit_mask) >> i) as u64;
            let z_bit = ((z & bit_mask) >> i) as u64;
            let t_bit = ((t & bit_mask) >> i) as u64;

            // Place bits at positions: 4*i, 4*i+1, 4*i+2, 4*i+3
            result |= x_bit << (4 * i);
            result |= y_bit << (4 * i + 1);
            result |= z_bit << (4 * i + 2);
            result |= t_bit << (4 * i + 3);
        }

        Morton4(result)
    }

    /// Decode Morton4 back to 4D block coordinates
    pub fn decode(&self) -> (u32, u32, u32, u32) {
        let mut x: u32 = 0;
        let mut y: u32 = 0;
        let mut z: u32 = 0;
        let mut t: u32 = 0;

        for i in 0..16 {
            let bit_mask = 1u64 << (4 * i);

            x |= (((self.0 & bit_mask) >> (4 * i)) as u32) << i;
            y |= (((self.0 & (bit_mask << 1)) >> (4 * i + 1)) as u32) << i;
            z |= (((self.0 & (bit_mask << 2)) >> (4 * i + 2)) as u32) << i;
            t |= (((self.0 & (bit_mask << 3)) >> (4 * i + 3)) as u32) << i;
        }

        (x, y, z, t)
    }

    /// Get the raw Morton code
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Create from raw u64
    pub fn from_u64(val: u64) -> Self {
        Morton4(val)
    }
}

/// A block of spatial data containing multiple entries
///
/// Similar to Minecraft chunks, blocks store data in a localized region
/// to improve cache locality and reduce fragmentation.
#[derive(Debug, Clone)]
pub struct Block<T> {
    /// Morton-encoded block coordinates
    pub morton: Morton4,
    /// Entries in this block
    pub entries: Vec<T>,
    /// Whether this block has been modified since last save
    pub dirty: bool,
}

impl<T> Block<T> {
    /// Create a new empty block
    pub fn new(morton: Morton4) -> Self {
        Self {
            morton,
            entries: Vec::new(),
            dirty: false,
        }
    }

    /// Add an entry to this block
    pub fn add(&mut self, entry: T) {
        self.entries.push(entry);
        self.dirty = true;
    }

    /// Get the number of entries in this block
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if block is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Mark block as clean (after save)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get block coordinates
    pub fn coords(&self) -> (u32, u32, u32, u32) {
        self.morton.decode()
    }
}

/// Block storage manager with LRU caching
///
/// Manages blocks in memory with automatic eviction using LRU policy.
/// Provides O(1) lookup by Morton code.
pub struct BlockStorage<T> {
    /// In-memory block cache (Morton4 -> Block)
    blocks: HashMap<Morton4, Block<T>>,
    /// LRU order tracking (most recent at end)
    lru_order: Vec<Morton4>,
    /// Maximum number of blocks in memory
    max_blocks: usize,
    /// Total entry count across all blocks
    total_entries: usize,
}

impl<T> BlockStorage<T> {
    /// Create a new block storage with given cache size
    pub fn new(max_blocks: usize) -> Self {
        Self {
            blocks: HashMap::with_capacity(max_blocks),
            lru_order: Vec::with_capacity(max_blocks),
            max_blocks,
            total_entries: 0,
        }
    }

    /// Insert an entry at the given coordinates
    ///
    /// Automatically creates or retrieves the appropriate block.
    pub fn insert(&mut self, x: u32, y: u32, z: u32, t: u32, entry: T) {
        let morton = Morton4::from_coords(x, y, z, t);

        // Get or create block
        let block = self.blocks.entry(morton).or_insert_with(|| {
            // Add to LRU order
            self.lru_order.push(morton);
            Block::new(morton)
        });

        // Update LRU order (move to end = most recent)
        if let Some(pos) = self.lru_order.iter().position(|&m| m == morton) {
            let m = self.lru_order.remove(pos);
            self.lru_order.push(m);
        }

        block.add(entry);
        self.total_entries += 1;

        // Evict if over capacity
        if self.blocks.len() > self.max_blocks {
            self.evict_lru();
        }
    }

    /// Get a block by its Morton code
    pub fn get_block(&self, morton: Morton4) -> Option<&Block<T>> {
        self.blocks.get(&morton)
    }

    /// Get a mutable block by its Morton code
    pub fn get_block_mut(&mut self, morton: Morton4) -> Option<&mut Block<T>> {
        // Update LRU order on access
        if let Some(pos) = self.lru_order.iter().position(|&m| m == morton) {
            let m = self.lru_order.remove(pos);
            self.lru_order.push(m);
        }
        self.blocks.get_mut(&morton)
    }

    /// Get all entries in a region
    ///
    /// Returns an iterator over all entries within the specified bounds.
    pub fn query_region(
        &self,
        min_x: u32,
        min_y: u32,
        min_z: u32,
        min_t: u32,
        max_x: u32,
        max_y: u32,
        max_z: u32,
        max_t: u32,
    ) -> RegionQuery<'_, T> {
        RegionQuery::new(self, min_x, min_y, min_z, min_t, max_x, max_y, max_z, max_t)
    }

    /// Evict the least recently used block
    fn evict_lru(&mut self) {
        if let Some(morton) = self.lru_order.first().copied() {
            if let Some(block) = self.blocks.remove(&morton) {
                self.total_entries -= block.len();
            }
            self.lru_order.remove(0);
        }
    }

    /// Get total entry count
    pub fn len(&self) -> usize {
        self.total_entries
    }

    /// Check if storage is empty
    pub fn is_empty(&self) -> bool {
        self.total_entries == 0
    }

    /// Get number of loaded blocks
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get all dirty blocks that need saving
    pub fn dirty_blocks(&self) -> Vec<&Block<T>> {
        self.blocks.values().filter(|b| b.dirty).collect()
    }

    /// Clear all blocks
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.lru_order.clear();
        self.total_entries = 0;
    }

    /// Get average entries per block
    pub fn avg_symbols_per_block(&self) -> f64 {
        if self.blocks.is_empty() {
            0.0
        } else {
            self.total_entries as f64 / self.blocks.len() as f64
        }
    }

    /// Get total symbol count (alias for len)
    pub fn symbol_count(&self) -> usize {
        self.total_entries
    }
}

/// Symbol entry for benchmark compatibility
#[cfg(feature = "benchmarks")]
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub fqn: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub t: f32,
}

/// Type alias for block storage with SymbolEntry (for benchmarks)
#[cfg(feature = "benchmarks")]
pub type SymbolBlockStorage = BlockStorage<SymbolEntry>;

#[cfg(feature = "benchmarks")]
impl SymbolBlockStorage {
    /// Insert a symbol with metadata (convenience method for benchmarks)
    pub fn insert_symbol(
        &mut self,
        fqn: String,
        name: String,
        kind: String,
        file_path: String,
        x: f32,
        y: f32,
        z: f32,
    ) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let entry = SymbolEntry {
            fqn,
            name,
            kind,
            file_path,
            x,
            y,
            z,
            t: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as f32,
        };

        // Convert float coordinates to block coordinates
        let bx = x as u32;
        let by = y as u32;
        let bz = z as u32;
        let bt = entry.t as u32;

        self.insert(bx, by, bz, bt, entry);
    }

    /// Find symbols in a region (convenience method for benchmarks)
    pub fn find_in_region(
        &self,
        min_x: f32,
        min_y: f32,
        min_z: f32,
        max_x: f32,
        max_y: f32,
        max_z: f32,
    ) -> Vec<&SymbolEntry> {
        let min_t = 0u32;
        let max_t = u32::MAX;

        self.query_region(
            min_x as u32,
            min_y as u32,
            min_z as u32,
            min_t,
            max_x as u32,
            max_y as u32,
            max_z as u32,
            max_t,
        )
        .collect()
    }

    /// Batch insert symbols (convenience method for benchmarks)
    pub fn insert_symbols_batch(
        &mut self,
        symbols: Vec<(String, String, String, String, f32, f32, f32)>,
    ) {
        for (fqn, name, kind, file, x, y, z) in symbols {
            self.insert_symbol(fqn, name, kind, file, x, y, z);
        }
    }
}

/// Iterator for region queries
pub struct RegionQuery<'a, T> {
    storage: &'a BlockStorage<T>,
    min_block_x: u32,
    min_block_y: u32,
    min_block_z: u32,
    min_block_t: u32,
    max_block_x: u32,
    max_block_y: u32,
    max_block_z: u32,
    max_block_t: u32,
    current_x: u32,
    current_y: u32,
    current_z: u32,
    current_t: u32,
    current_block_idx: usize,
}

impl<'a, T> RegionQuery<'a, T> {
    fn new(
        storage: &'a BlockStorage<T>,
        min_x: u32,
        min_y: u32,
        min_z: u32,
        min_t: u32,
        max_x: u32,
        max_y: u32,
        max_z: u32,
        max_t: u32,
    ) -> Self {
        let min_block_x = min_x / Morton4::BLOCK_SIZE;
        let min_block_y = min_y / Morton4::BLOCK_SIZE;
        let min_block_z = min_z / Morton4::BLOCK_SIZE;
        let min_block_t = min_t / Morton4::BLOCK_SIZE;
        let max_block_x = max_x / Morton4::BLOCK_SIZE;
        let max_block_y = max_y / Morton4::BLOCK_SIZE;
        let max_block_z = max_z / Morton4::BLOCK_SIZE;
        let max_block_t = max_t / Morton4::BLOCK_SIZE;

        Self {
            storage,
            min_block_x,
            min_block_y,
            min_block_z,
            min_block_t,
            max_block_x,
            max_block_y,
            max_block_z,
            max_block_t,
            current_x: min_block_x,
            current_y: min_block_y,
            current_z: min_block_z,
            current_t: min_block_t,
            current_block_idx: 0,
        }
    }
}

impl<'a, T> Iterator for RegionQuery<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Get current block
            let morton = Morton4::encode(
                self.current_x,
                self.current_y,
                self.current_z,
                self.current_t,
            );

            if let Some(block) = self.storage.get_block(morton) {
                if self.current_block_idx < block.entries.len() {
                    let entry = &block.entries[self.current_block_idx];
                    self.current_block_idx += 1;
                    return Some(entry);
                }
            }

            // Move to next block
            self.current_block_idx = 0;
            self.current_t += 1;

            if self.current_t > self.max_block_t {
                self.current_t = self.min_block_t;
                self.current_z += 1;

                if self.current_z > self.max_block_z {
                    self.current_z = self.min_block_z;
                    self.current_y += 1;

                    if self.current_y > self.max_block_y {
                        self.current_y = self.min_block_y;
                        self.current_x += 1;

                        if self.current_x > self.max_block_x {
                            return None;
                        }
                    }
                }
            }
        }
    }
}

/// Batch inserter for optimized bulk operations
///
/// Groups entries by block and inserts them in batches for better performance.
pub struct BatchInserter<T> {
    /// Pending entries grouped by Morton code
    pending: HashMap<Morton4, Vec<T>>,
    /// Batch size threshold for flushing
    batch_size: usize,
}

impl<T> BatchInserter<T> {
    /// Create a new batch inserter
    pub fn new(batch_size: usize) -> Self {
        Self {
            pending: HashMap::new(),
            batch_size,
        }
    }

    /// Add an entry to the batch
    pub fn add(&mut self, x: u32, y: u32, z: u32, t: u32, entry: T) {
        let morton = Morton4::from_coords(x, y, z, t);
        self.pending.entry(morton).or_default().push(entry);
    }

    /// Flush all pending entries to storage
    pub fn flush(&mut self, storage: &mut BlockStorage<T>) -> Result<usize> {
        let mut total = 0;

        for (morton, entries) in self.pending.drain() {
            let block = storage.blocks.entry(morton).or_insert_with(|| {
                storage.lru_order.push(morton);
                Block::new(morton)
            });

            let count = entries.len();
            block.entries.extend(entries);
            block.dirty = true;
            storage.total_entries += count;
            total += count;
        }

        Ok(total)
    }

    /// Get number of pending entries
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// Check if batch should be flushed
    pub fn should_flush(&self) -> bool {
        self.pending_count() >= self.batch_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morton4_encode_decode() {
        let test_cases = [
            (0u32, 0u32, 0u32, 0u32),
            (1u32, 0u32, 0u32, 0u32),
            (0u32, 1u32, 0u32, 0u32),
            (0u32, 0u32, 1u32, 0u32),
            (0u32, 0u32, 0u32, 1u32),
            (15u32, 15u32, 15u32, 15u32),
            (16u32, 16u32, 16u32, 16u32),
            (100u32, 200u32, 300u32, 400u32),
        ];

        for (x, y, z, t) in test_cases {
            let morton = Morton4::encode(x, y, z, t);
            let (dx, dy, dz, dt) = morton.decode();
            assert_eq!(
                (x, y, z, t),
                (dx, dy, dz, dt),
                "Failed for coordinates ({}, {}, {}, {})",
                x,
                y,
                z,
                t
            );
        }
    }

    #[test]
    fn test_morton4_from_coords() {
        // Test that coordinates are correctly divided by BLOCK_SIZE
        let m1 = Morton4::from_coords(0, 0, 0, 0);
        let m2 = Morton4::from_coords(15, 15, 15, 15);
        assert_eq!(
            m1.as_u64(),
            m2.as_u64(),
            "Same block should have same Morton code"
        );

        let m3 = Morton4::from_coords(16, 0, 0, 0);
        assert_ne!(
            m1.as_u64(),
            m3.as_u64(),
            "Different blocks should have different codes"
        );
    }

    #[test]
    fn test_block_operations() {
        let morton = Morton4::from_coords(0, 0, 0, 0);
        let mut block = Block::<String>::new(morton);

        assert!(block.is_empty());
        block.add("test".to_string());
        assert_eq!(block.len(), 1);
        assert!(block.dirty);

        block.mark_clean();
        assert!(!block.dirty);
    }

    #[test]
    fn test_block_storage_insert() {
        let mut storage = BlockStorage::<u64>::new(10);

        // Insert entries in same block
        for i in 0..5 {
            storage.insert(0, 0, 0, 0, i);
        }

        assert_eq!(storage.len(), 5);
        assert_eq!(storage.block_count(), 1);

        // Insert in different block
        storage.insert(100, 0, 0, 0, 100);
        assert_eq!(storage.block_count(), 2);
    }

    #[test]
    fn test_block_storage_lru_eviction() {
        let mut storage = BlockStorage::<u64>::new(2);

        // Insert into 3 different blocks (only 2 should remain)
        storage.insert(0, 0, 0, 0, 1); // Block (0,0,0,0)
        storage.insert(16, 0, 0, 0, 2); // Block (1,0,0,0)
        storage.insert(32, 0, 0, 0, 3); // Block (2,0,0,0)

        assert_eq!(storage.block_count(), 2);
        assert_eq!(storage.len(), 2); // One entry evicted
    }

    #[test]
    fn test_region_query() {
        let mut storage = BlockStorage::<u64>::new(10);

        // Insert entries in a grid
        for x in 0..32u32 {
            for y in 0..4u32 {
                storage.insert(x, y, 0, 0, (x * 100 + y) as u64);
            }
        }

        // Query small region
        let results: Vec<_> = storage
            .query_region(0, 0, 0, 0, 15, 3, 0, 0)
            .copied()
            .collect();

        // Should get entries from blocks (0,0,0,0) and (0,0,0,0) only
        assert!(!results.is_empty());

        // Query larger region
        let results: Vec<_> = storage
            .query_region(0, 0, 0, 0, 31, 3, 0, 0)
            .copied()
            .collect();

        assert_eq!(results.len(), 128); // 32 * 4
    }

    #[test]
    fn test_batch_inserter() {
        let mut storage = BlockStorage::<u64>::new(10);
        let mut batch = BatchInserter::new(5);

        // Add entries
        for i in 0..10u64 {
            batch.add((i * 16) as u32, 0, 0, 0, i);
        }

        assert_eq!(batch.pending_count(), 10);

        // Flush to storage
        batch.flush(&mut storage).unwrap();

        assert_eq!(storage.len(), 10);
        assert_eq!(storage.block_count(), 10);
    }

    #[test]
    fn test_clustered_vs_scattered() {
        // Test that clustered insertion is more efficient
        let mut clustered = BlockStorage::<u64>::new(100);
        let mut scattered = BlockStorage::<u64>::new(100);

        // Clustered: all in one block
        for i in 0..100 {
            clustered.insert(0, 0, 0, 0, i);
        }

        // Scattered: each in different block
        for i in 0..100 {
            scattered.insert((i * 16) as u32, 0, 0, 0, i);
        }

        assert_eq!(clustered.block_count(), 1);
        assert_eq!(scattered.block_count(), 100);
    }
}
