use xxhash_rust::xxh3::xxh3_64;

/// Fast non-cryptographic hash of file content for change detection.
pub fn compute_hash(content: &[u8]) -> u64 {
    xxh3_64(content)
}

/// Quick change check using mtime + size. Returns true if the file
/// appears unchanged based on metadata alone.
pub fn metadata_matches(
    cached_mtime: u64,
    cached_size: u64,
    current_mtime: u64,
    current_size: u64,
) -> bool {
    cached_mtime == current_mtime && cached_size == current_size
}
