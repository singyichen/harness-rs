use std::io::Read;

pub mod post_tool_use;
pub mod prompt_nudge;
pub mod session_start;
pub mod stop_gate;

/// Hook engine entry point. Iron rule: any internal error results in
/// allowing the action (exit 0, no output).
pub fn run(event: &str) -> i32 {
    std::panic::set_hook(Box::new(|_| {})); // silence panic messages
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload: serde_json::Value =
        serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(event, &payload)
    }));
    if let Ok(Some(output)) = result {
        println!("{output}");
    }
    0
}

fn dispatch(event: &str, payload: &serde_json::Value) -> Option<String> {
    match event {
        "post-tool" => post_tool_use::run(payload),
        "session-start" => session_start::run(payload),
        "user-prompt" => prompt_nudge::run(payload),
        "stop" => stop_gate::run(payload),
        _ => None,
    }
}
