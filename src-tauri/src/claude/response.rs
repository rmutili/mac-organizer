//! Extracts the plan from a Messages API response body.

use serde_json::Value;

use super::plan::RawPlan;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ResponseError {
    #[error("Claude's plan was cut off because it was too long. Try a smaller batch.")]
    Truncated,
    #[error("Claude didn't return a plan.")]
    MissingPlan,
    #[error("Claude returned a plan in an unexpected format: {0}")]
    Malformed(String),
}

pub fn parse_plan(body: &Value) -> Result<RawPlan, ResponseError> {
    if body["stop_reason"] == "max_tokens" {
        return Err(ResponseError::Truncated);
    }
    let input = body["content"]
        .as_array()
        .into_iter()
        .flatten()
        // tool_choice forces a single tool, so the first tool call is the answer.
        .find(|block| block["type"] == "tool_use")
        .map(|block| block["input"].clone())
        .ok_or(ResponseError::MissingPlan)?;
    serde_json::from_value(input).map_err(|e| ResponseError::Malformed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(stop_reason: &str, content: Value) -> Value {
        json!({ "type": "message", "stop_reason": stop_reason, "content": content })
    }

    #[test]
    fn extracts_tool_input() {
        let body = response(
            "tool_use",
            json!([
                { "type": "text", "text": "Here is the plan" },
                { "type": "tool_use", "id": "t1", "name": "propose_plan", "input": {
                    "folders": [{ "name": "Docs", "description": "d" }],
                    "assignments": [{ "item_id": 0, "folder": "Docs", "reason": "pdf" }],
                    "deletion_candidates": [{ "item_id": 1, "reason": "old", "confidence": "high" }]
                }}
            ]),
        );

        let plan = parse_plan(&body).unwrap();

        assert_eq!(plan.folders[0].name, "Docs");
        assert_eq!(plan.assignments[0].item_id, 0);
        assert_eq!(plan.deletion_candidates.len(), 1);
    }

    #[test]
    fn truncated_responses_are_an_error_even_with_partial_tool_input() {
        let body = response(
            "max_tokens",
            json!([{ "type": "tool_use", "name": "propose_plan", "input": {} }]),
        );

        assert_eq!(parse_plan(&body), Err(ResponseError::Truncated));
    }

    #[test]
    fn missing_tool_call_is_an_error() {
        let body = response("end_turn", json!([{ "type": "text", "text": "no" }]));

        assert_eq!(parse_plan(&body), Err(ResponseError::MissingPlan));
    }

    #[test]
    fn wrong_field_types_are_malformed() {
        let body = response(
            "tool_use",
            json!([{ "type": "tool_use", "name": "propose_plan", "input": {
                "folders": "Docs", "assignments": [], "deletion_candidates": []
            }}]),
        );

        assert!(matches!(
            parse_plan(&body),
            Err(ResponseError::Malformed(_))
        ));
    }
}
