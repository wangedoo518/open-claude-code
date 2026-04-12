//! Manual smoke test for an Anthropic-compatible gateway.
//!
//! This test is `#[ignore]` by default because it requires a real
//! gateway endpoint and a real API key supplied through environment
//! variables.
//!
//! Required env vars:
//!
//!   1. `GATEWAY_BASE_URL`
//!   2. `GATEWAY_API_KEY`
//!   3. Optional: `GATEWAY_MODEL`
//!
//! Example:
//!
//!   export GATEWAY_BASE_URL="http://127.0.0.1:8080"
//!   export GATEWAY_API_KEY="sk-test-..."
//!   export GATEWAY_MODEL="claude-sonnet-4-5"
//!   cargo test -p api --test compatible_gateway_manual_smoke -- --ignored --nocapture
//!
//! Why this test exists: compatible gateways are expected to return a
//! valid Anthropic Messages response with at least one non-empty text
//! block. If a conversion layer regresses and drops `content[].text`,
//! this smoke test fails loudly.

use std::env;

use api::{AnthropicClient, AuthSource, InputMessage, MessageRequest};

const EXPECTED_MODEL: &str = "claude-sonnet-4-5";

#[tokio::test]
#[ignore = "hits a real gateway; run manually with `cargo test -- --ignored`"]
async fn compatible_gateway_returns_non_empty_text_content() {
    let base_url = env::var("GATEWAY_BASE_URL")
        .expect("GATEWAY_BASE_URL env var must be set");
    let api_key = env::var("GATEWAY_API_KEY")
        .expect("GATEWAY_API_KEY env var must be set");
    let model = env::var("GATEWAY_MODEL").unwrap_or_else(|_| EXPECTED_MODEL.to_string());

    let client = AnthropicClient::from_auth(AuthSource::ApiKey(api_key))
        .with_base_url(base_url);

    let request = MessageRequest {
        model,
        max_tokens: 40,
        messages: vec![InputMessage::user_text(
            "Reply with exactly these five words: compatible gateway smoke test works",
        )],
        system: None,
        tools: None,
        tool_choice: None,
        stream: false,
    };

    let response = client
        .send_message(&request)
        .await
        .expect("gateway should accept the request and return a valid Anthropic response");

    // === Layer 1: response envelope ===
    assert_eq!(response.kind, "message", "response envelope should have type=message");
    assert_eq!(response.role, "assistant", "response role should be assistant");
    assert!(
        !response.id.is_empty(),
        "response id must be non-empty (got {:?})",
        response.id
    );

    // === Layer 2: content blocks must exist AND at least one must have real text ===
    assert!(
        !response.content.is_empty(),
        "response.content must not be empty — this is the Bug A regression indicator (\
         content[].text was getting lost in the Responses → Anthropic conversion layer)"
    );

    let total_text: String = response
        .content
        .iter()
        .filter_map(|block| match block {
            api::OutputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(
        !total_text.trim().is_empty(),
        "sum of all text blocks must be non-empty — Bug A symptom: \
         blocks exist but their text fields are empty strings. Full response: {response:?}"
    );

    println!("✓ gateway returned real text ({}): {total_text:?}", total_text.len());

    // === Layer 3: usage counters must reflect real upstream call ===
    assert!(
        response.usage.input_tokens > 0,
        "usage.input_tokens should be > 0 (real upstream call); got {}",
        response.usage.input_tokens
    );
    assert!(
        response.usage.output_tokens > 0,
        "usage.output_tokens should be > 0 (real upstream call); got {}",
        response.usage.output_tokens
    );

    println!(
        "✓ usage: input={} output={} total={}",
        response.usage.input_tokens,
        response.usage.output_tokens,
        response.total_tokens()
    );
}
