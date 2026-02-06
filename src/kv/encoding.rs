// KV Encoding Helpers for Native V2 Backend
//
// This module provides encoding/decoding functions for storing complex data
// structures in the Native V2 backend's KV store.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_symbol_ids_roundtrip() {
        // Test empty array
        let empty: Vec<i64> = vec![];
        let encoded = encode_symbol_ids(&empty);
        let decoded = decode_symbol_ids(&encoded);
        assert_eq!(decoded.as_slice(), empty.as_slice());

        // Test single ID
        let single = vec![12345i64];
        let encoded = encode_symbol_ids(&single);
        let decoded = decode_symbol_ids(&encoded);
        assert_eq!(decoded.as_slice(), single.as_slice());

        // Test multiple IDs
        let multiple = vec![1i64, 2, 3, 1000, -1, -9999];
        let encoded = encode_symbol_ids(&multiple);
        let decoded = decode_symbol_ids(&encoded);
        assert_eq!(decoded.as_slice(), multiple.as_slice());

        // Test large values
        let large = vec![i64::MAX, i64::MIN, 0];
        let encoded = encode_symbol_ids(&large);
        let decoded = decode_symbol_ids(&encoded);
        assert_eq!(decoded.as_slice(), large.as_slice());
    }

    #[test]
    fn test_encode_symbol_ids_empty() {
        let ids: Vec<i64> = vec![];
        let encoded = encode_symbol_ids(&ids);
        assert!(encoded.is_empty());
    }

    #[test]
    fn test_encode_symbol_ids_non_empty() {
        let ids = vec![1i64, 2, 3];
        let encoded = encode_symbol_ids(&ids);
        // Each i64 is 8 bytes, so 3 IDs = 24 bytes
        assert_eq!(encoded.len(), 24);
    }

    #[test]
    fn test_decode_symbol_ids_invalid_length() {
        // Test with incomplete byte sequence (not multiple of 8)
        let incomplete = vec![1u8, 2, 3]; // 3 bytes, not 8
        let decoded = decode_symbol_ids(&incomplete);
        // Should handle gracefully - incomplete bytes are ignored
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_encode_decode_preserves_values() {
        let original = vec![
            0i64,
            1,
            -1,
            1000000000,
            -1000000000,
            i64::MAX,
            i64::MIN,
        ];
        let encoded = encode_symbol_ids(&original);
        let decoded = decode_symbol_ids(&encoded);
        assert_eq!(decoded.as_slice(), original.as_slice());
    }
}

/// Encode a slice of SymbolId (i64) values to a byte vector for KV storage.
///
/// Uses little-endian encoding (8 bytes per i64) with flat_map for efficient
/// transformation. This encoding is compact and allows O(1) lookup when stored
/// under indexed keys.
///
/// # Arguments
/// * `ids` - Slice of i64 symbol IDs to encode
///
/// # Returns
/// Vec<u8> containing the concatenated little-endian representation
///
/// # Example
/// ```ignore
/// let ids = vec![1i64, 2, 3];
/// let bytes = encode_symbol_ids(&ids);
/// assert_eq!(bytes.len(), 24); // 3 IDs * 8 bytes each
/// ```
pub fn encode_symbol_ids(ids: &[i64]) -> Vec<u8> {
    ids.iter()
        .flat_map(|id| id.to_le_bytes().to_vec())
        .collect()
}

/// Decode a byte vector back to a Vec<i64> of SymbolId values.
///
/// Expects little-endian encoded i64 values (8 bytes each). Any trailing
/// incomplete bytes (not a multiple of 8) are silently ignored.
///
/// # Arguments
/// * `bytes` - Byte slice containing encoded symbol IDs
///
/// # Returns
/// Vec<i64> of decoded symbol IDs
///
/// # Example
/// ```ignore
/// let ids = vec![1i64, 2, 3];
/// let bytes = encode_symbol_ids(&ids);
/// let decoded = decode_symbol_ids(&bytes);
/// assert_eq!(decoded, ids);
/// ```
pub fn decode_symbol_ids(bytes: &[u8]) -> Vec<i64> {
    bytes
        .chunks_exact(8)
        .map(|chunk| {
            let arr: [u8; 8] = chunk.try_into().unwrap_or([0u8; 8]);
            i64::from_le_bytes(arr)
        })
        .collect()
}
