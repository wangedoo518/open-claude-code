//! OpenAI-compatible tool schema adapter.
//!
//! Converts internal `ToolSpec` (which uses Anthropic-style `input_schema`)
//! into OpenAI ChatCompletions `tools` array entries.
//!
//! Schema body is identical (both are JSON Schema). Only outer wrapper differs:
//!
//! Anthropic:
//!   { "name", "description", "input_schema": {...} }
//!
//! OpenAI:
//!   { "type": "function",
//!     "function": { "name", "description", "parameters": {...} } }
//!
//! This module is provider-adapter logic; it does not validate schemas.
//! ToolSpec is assumed to be well-formed (statically defined or validated
//! at MCP discovery time).

use serde_json::Value;
use tools::ToolSpec;

/// Convert a single `ToolSpec` into an OpenAI ChatCompletions tool entry.
///
/// Returns a `serde_json::Value` representing one element of the `tools` array
/// in an OpenAI chat completions request.
///
/// # Example
///
/// ```ignore
/// let spec = ToolSpec {
///     name: "get_weather",
///     description: "Get weather for a city",
///     input_schema: serde_json::json!({
///         "type": "object",
///         "properties": { "city": { "type": "string" } },
///         "required": ["city"]
///     }),
///     required_permission: PermissionMode::ReadOnly,
/// };
/// let openai_tool = to_openai_function_tool(&spec);
/// // {
/// //   "type": "function",
/// //   "function": {
/// //     "name": "get_weather",
/// //     "description": "Get weather for a city",
/// //     "parameters": { ... same JSON Schema ... }
/// //   }
/// // }
/// ```
pub fn to_openai_function_tool(spec: &ToolSpec) -> Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.input_schema,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::PermissionMode;

    fn make_spec(name: &'static str, description: &'static str, input_schema: Value) -> ToolSpec {
        ToolSpec {
            name,
            description,
            input_schema,
            required_permission: PermissionMode::ReadOnly,
        }
    }

    #[test]
    fn preserves_name_description_and_schema() {
        let spec = make_spec(
            "web_search",
            "Search the web for information",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        );

        let result = to_openai_function_tool(&spec);

        assert_eq!(result["type"], "function");
        assert_eq!(result["function"]["name"], "web_search");
        assert_eq!(
            result["function"]["description"],
            "Search the web for information"
        );

        let params = &result["function"]["parameters"];
        assert_eq!(params["type"], "object");
        assert_eq!(params["properties"]["query"]["type"], "string");
        assert_eq!(params["required"][0], "query");
    }

    #[test]
    fn handles_empty_schema() {
        let spec = make_spec("noop", "Does nothing", serde_json::json!({}));

        let result = to_openai_function_tool(&spec);

        assert_eq!(result["function"]["name"], "noop");
        assert!(result["function"]["parameters"].is_object());
        assert_eq!(
            result["function"]["parameters"].as_object().unwrap().len(),
            0
        );
    }

    #[test]
    fn handles_complex_nested_schema() {
        let spec = make_spec(
            "execute_query",
            "Execute a database query",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "options": {
                        "type": "object",
                        "properties": {
                            "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                            "fields": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    }
                },
                "required": ["query"]
            }),
        );

        let result = to_openai_function_tool(&spec);

        let params = &result["function"]["parameters"];
        assert_eq!(
            params["properties"]["options"]["properties"]["limit"]["maximum"],
            1000
        );
        assert_eq!(
            params["properties"]["options"]["properties"]["fields"]["items"]["type"],
            "string"
        );
    }

    #[test]
    fn produces_openai_compatible_top_level_keys() {
        let spec = make_spec("noop", "Test", serde_json::json!({}));
        let result = to_openai_function_tool(&spec);

        let obj = result.as_object().expect("result must be a JSON object");
        let keys: Vec<&String> = obj.keys().collect();

        // OpenAI spec requires exactly these top-level keys.
        assert_eq!(
            keys.len(),
            2,
            "top-level should have exactly 2 keys, got: {:?}",
            keys
        );
        assert!(obj.contains_key("type"));
        assert!(obj.contains_key("function"));
    }

    #[test]
    fn function_object_has_required_fields() {
        let spec = make_spec("noop", "Test", serde_json::json!({}));
        let result = to_openai_function_tool(&spec);

        let func = result["function"]
            .as_object()
            .expect("function must be object");
        assert!(func.contains_key("name"));
        assert!(func.contains_key("description"));
        assert!(func.contains_key("parameters"));
    }
}
