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

    #[test]
    fn test_encode_decode_json_roundtrip() {
        // Test with a simple struct
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct TestStruct {
            name: String,
            value: i64,
        }

        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let encoded = encode_json(&original).unwrap();
        let decoded: TestStruct = decode_json(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_json_vec() {
        let original = vec![1i64, 2, 3, 4, 5];

        let encoded = encode_json(&original).unwrap();
        let decoded: Vec<i64> = decode_json(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_json_complex() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct ComplexStruct {
            id: i64,
            name: String,
            tags: Vec<String>,
            nested: NestedStruct,
        }

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct NestedStruct {
            flag: bool,
            value: Option<i64>,
        }

        let original = ComplexStruct {
            id: 123,
            name: "test_complex".to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            nested: NestedStruct {
                flag: true,
                value: Some(999),
            },
        };

        let encoded = encode_json(&original).unwrap();
        let decoded: ComplexStruct = decode_json(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn test_encode_decode_json_empty() {
        let empty: Vec<i64> = vec![];
        let encoded = encode_json(&empty).unwrap();
        let decoded: Vec<i64> = decode_json(&encoded).unwrap();

        assert_eq!(decoded, empty);
        assert!(encoded.is_empty() || encoded == b"[]");
    }

    #[test]
    fn test_encode_json_invalid_utf8() {
        // Test that encode_json handles non-string types correctly
        let data: Vec<u8> = vec![0xFF, 0xFE, 0xFD];
        let encoded = encode_json(&data).unwrap();

        // Should produce valid JSON (base64 or similar for bytes)
        let decoded: Vec<u8> = decode_json(&encoded).unwrap();
        assert_eq!(decoded, data);
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

/// Encode a serializable value to JSON bytes for KV storage.
///
/// Generic encoding function for any type implementing serde::Serialize.
/// Uses serde_json for compact, human-readable encoding.
///
/// # Type Parameters
/// * `T` - Type implementing serde::Serialize (can be unsized like slices)
///
/// # Arguments
/// * `value` - Reference to value to encode
///
/// # Returns
/// Result<Vec<u8>> containing JSON bytes
///
/// # Errors
/// Returns error if serialization fails
///
/// # Example
/// ```ignore
/// let data = vec![1, 2, 3];
/// let bytes = encode_json(&data)?;
/// ```
pub fn encode_json<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| anyhow::anyhow!("JSON encoding failed: {}", e))
}

/// Decode JSON bytes back to a deserializable type.
///
/// Generic decoding function for any type implementing serde::de::DeserializeOwned.
/// Uses serde_json for deserialization.
///
/// # Arguments
/// * `bytes` - Byte slice containing JSON data
///
/// # Returns
/// Result<T> containing decoded value
///
/// # Errors
/// Returns error if deserialization fails
///
/// # Example
/// ```ignore
/// let data: Vec<i64> = decode_json(&bytes)?;
/// ```
pub fn decode_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> anyhow::Result<T> {
    serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("JSON decoding failed: {}", e))
}

/// Encode CFG blocks for KV storage.
///
/// Convenience wrapper around encode_json for slices of serializable types.
/// This function is generic to avoid direct dependencies on private types.
///
/// # Type Parameters
/// * `T` - Type implementing serde::Serialize (e.g., CfgBlock)
///
/// # Arguments
/// * `blocks` - Slice of values to encode
///
/// # Returns
/// Result<Vec<u8>> containing JSON bytes
///
/// # Errors
/// Returns error if JSON serialization fails
///
/// # Example
/// ```ignore
/// use crate::graph::schema::CfgBlock;
/// let blocks = vec![CfgBlock { ... }];
/// let bytes = encode_cfg_blocks(&blocks)?;
/// ```
pub fn encode_cfg_blocks<T: serde::Serialize>(blocks: &[T]) -> anyhow::Result<Vec<u8>> {
    encode_json(blocks)
}

/// Decode CFG blocks from KV storage.
///
/// Convenience wrapper around decode_json for deserializable types.
/// This function is generic to avoid direct dependencies on private types.
///
/// # Type Parameters
/// * `T` - Type implementing serde::de::DeserializeOwned (e.g., CfgBlock)
///
/// # Arguments
/// * `bytes` - Byte slice containing JSON data
///
/// # Returns
/// Result<Vec<T>> containing decoded values
///
/// # Errors
/// Returns error if JSON deserialization fails
///
/// # Example
/// ```ignore
/// use crate::graph::schema::CfgBlock;
/// let blocks: Vec<CfgBlock> = decode_cfg_blocks(&bytes)?;
/// ```
pub fn decode_cfg_blocks<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> anyhow::Result<Vec<T>> {
    decode_json(bytes)
}

/// Encode AST nodes for KV storage.
///
/// Convenience wrapper around encode_json for slices of serializable types.
/// This function is generic to avoid direct dependencies on private types.
///
/// # Type Parameters
/// * `T` - Type implementing serde::Serialize (e.g., AstNode)
///
/// # Arguments
/// * `nodes` - Slice of values to encode
///
/// # Returns
/// Result<Vec<u8>> containing JSON bytes
///
/// # Errors
/// Returns error if JSON serialization fails
///
/// # Example
/// ```ignore
/// use crate::graph::ast_node::AstNode;
/// let nodes = vec![AstNode { ... }];
/// let bytes = encode_ast_nodes(&nodes)?;
/// ```
pub fn encode_ast_nodes<T: serde::Serialize>(nodes: &[T]) -> anyhow::Result<Vec<u8>> {
    encode_json(nodes)
}

/// Decode AST nodes from KV storage.
///
/// Convenience wrapper around decode_json for deserializable types.
/// This function is generic to avoid direct dependencies on private types.
///
/// # Type Parameters
/// * `T` - Type implementing serde::de::DeserializeOwned (e.g., AstNode)
///
/// # Arguments
/// * `bytes` - Byte slice containing JSON data
///
/// # Returns
/// Result<Vec<T>> containing decoded values
///
/// # Errors
/// Returns error if JSON deserialization fails
///
/// # Example
/// ```ignore
/// use crate::graph::ast_node::AstNode;
/// let nodes: Vec<AstNode> = decode_ast_nodes(&bytes)?;
/// ```
pub fn decode_ast_nodes<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> anyhow::Result<Vec<T>> {
    decode_json(bytes)
}
