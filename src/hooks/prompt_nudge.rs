use serde_json::Value;

pub fn run(_payload: &Value) -> Option<String> {
    Some(
        "🧭 Harness: Observe first (gather evidence) → state assumptions; functional changes need fail-then-pass evidence; adversarially review major conclusions before trusting them."
            .to_string(),
    )
}
