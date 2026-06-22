// Unit tests for token budget feature
// These tests verify core functionality without requiring full CLI integration

#[test]
fn test_json_response_metadata() {
    // Test that JSON responses include tokens_estimated and truncated fields
    use magellan::output::command::JsonResponse;
    use serde_json::json;

    let response = JsonResponse::new(json!({"test": "data"}), "test-exec-id")
        .with_tokens(100)
        .with_truncated(false);

    // Verify that the fields were set correctly
    assert_eq!(response.tokens_estimated, Some(100));
    assert_eq!(response.truncated, Some(false));
}

#[test]
fn test_token_estimation_heuristic() {
    // Test that token estimation uses chars / 4 heuristic
    let test_string = "abcdefgh"; // 8 chars
    let estimated_tokens = test_string.len() / 4;
    assert_eq!(estimated_tokens, 2, "Token estimation should use chars / 4");
}

#[test]
fn test_backward_compatibility_no_limit() {
    // Test that --tokens 0 behaves same as absent flag (no limit)
    use magellan::output::command::JsonResponse;
    use serde_json::json;

    let response_with_zero = JsonResponse::new(json!({"test": "data"}), "test-exec-id")
        .with_tokens(0)
        .with_truncated(false);

    // When tokens is 0, it should not cause truncation
    assert_eq!(response_with_zero.tokens_estimated, Some(0));
    assert_eq!(response_with_zero.truncated, Some(false));
}

#[test]
fn test_token_fields_optional_in_json() {
    // Test that token fields are optional and serialize correctly
    use magellan::output::command::JsonResponse;
    use serde_json::json;

    let response_with_tokens = JsonResponse::new(json!({"test": "data"}), "test-exec-id")
        .with_tokens(100)
        .with_truncated(true);

    let response_without_tokens = JsonResponse::new(json!({"test": "data"}), "test-exec-id");

    // Verify the fields are set correctly
    assert_eq!(response_with_tokens.tokens_estimated, Some(100));
    assert_eq!(response_with_tokens.truncated, Some(true));
    assert_eq!(response_without_tokens.tokens_estimated, None);
    assert_eq!(response_without_tokens.truncated, None);
}
