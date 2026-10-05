use serde::Serialize;

/// Machine envelope for --json mode. Stdout is JSON only.
#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub data: T,
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, Default)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub fix: String,
}

pub fn print_json<T: Serialize>(ok: bool, data: T) {
    let env = Envelope {
        ok,
        data,
        error: ErrorBody::default(),
    };
    println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
}

pub fn print_json_error(code: &str, message: &str, fix: &str) {
    let env: Envelope<serde_json::Value> = Envelope {
        ok: false,
        data: serde_json::Value::Null,
        error: ErrorBody {
            code: code.to_string(),
            message: message.to_string(),
            fix: fix.to_string(),
        },
    };
    println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
}

/// Exit code mapping from plan.md Section 16.
pub fn exit_code_for(code: &str) -> i32 {
    match code {
        "E_ARGS" => 2,
        "E_MODEL_MISSING" => 3,
        "E_OFFLINE" => 4,
        "E_TOOL_DENIED" => 5,
        _ => 6,
    }
}
