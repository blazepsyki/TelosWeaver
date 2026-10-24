//! 마스터 프롬프트와 응답 해석.
//!
//! 마스터는 최종 응답에 ```json 코드 블록 하나로 결정을 냅니다. Provider 전용 구조화 출력 기능에 기대지 않아
//! 마스터 Provider를 바꿔도 같은 형식을 씁니다 (MASTER-01).

use serde::Deserialize;
use serde_json::Value;
use telos_schema::{JsonObject, MasterDecisionKind, TaskSpec};

/// 마스터가 정하는 부분입니다. 나머지 결정 필드는 Core가 채웁니다.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MasterOutput {
    pub kind: MasterDecisionKind,
    pub payload: JsonObject,
    pub rationale: String,
}

/// 응답 텍스트에서 마지막 ```json 블록(없으면 마지막 최상위 JSON 객체)을 결정으로 해석합니다.
pub fn parse_master_output(text: &str) -> Result<MasterOutput, String> {
    let candidate = last_json_block(text)
        .or_else(|| last_object(text))
        .ok_or_else(|| "no JSON decision in the master response".to_owned())?;
    let value: Value =
        serde_json::from_str(candidate).map_err(|e| format!("decision is not valid JSON: {e}"))?;
    serde_json::from_value(value).map_err(|e| format!("decision has an invalid shape: {e}"))
}

fn last_json_block(text: &str) -> Option<&str> {
    let start = text.rfind("```json")? + "```json".len();
    let end = text[start..].find("```")? + start;
    Some(text[start..end].trim())
}

/// 마지막 `}`에서 짝이 맞는 `{`까지를 찾습니다. 문자열 안의 괄호는 건너뜁니다.
fn last_object(text: &str) -> Option<&str> {
    let end = text.rfind('}')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut i = end as isize;
    while i >= 0 {
        let c = bytes[i as usize];
        let escaped = i > 0 && bytes[i as usize - 1] == b'\\';
        match c {
            b'"' if !escaped => in_string = !in_string,
            b'}' if !in_string => depth += 1,
            b'{' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[i as usize..=end]);
                }
            }
            _ => {}
        }
        i -= 1;
    }
    None
}

/// 마스터 Turn 프롬프트입니다. `state`는 현재 Plan 상태 JSON, `message`는 목표 또는 직전 집행 결과입니다.
pub fn master_prompt(message: &str, state: &Value, worker_template: &TaskSpec) -> String {
    // 실제 haiku 마스터가 "틀 복사" 지시를 틀 전체를 objective에 넣는 것으로 오해한 사례가 있어,
    // 바꿀 두 자리를 표시한 완성형 Task 객체를 그대로 보여 줍니다.
    let mut example = worker_template.clone();
    example.id = "<NEW_TASK_ID>".into();
    example.objective = "<OBJECTIVE_AS_PLAIN_TEXT>".into();
    let template = serde_json::to_string(&example).expect("task template serializes");
    format!(
        r#"You are the master controller of a TelosWeaver plan. You do not use tools and you do not do the work yourself.
Decide exactly ONE next step and reply with exactly one JSON object inside a ```json code block. Do not write anything after the block.

JSON shape: {{"kind": "<kind>", "payload": {{...}}, "rationale": "<one short sentence>"}}

Kinds and payloads:
- create_tasks: {{"tasks": [<task>, ...]}}. Each <task> is this exact JSON object with only the two placeholders replaced: "<NEW_TASK_ID>" by a new short id and "<OBJECTIVE_AS_PLAIN_TEXT>" by a plain-text instruction for the worker (a string, never an object). Keep every other field unchanged unless the message tells you otherwise:
  {template}
- request_run: {{"taskId": "<id>", "taskRevision": <revision>, "session": {{"mode": "fresh"}}}}. Only for a task whose status is "Ready".
- request_cancel: {{"taskId": "<id>", "reason": "<why>", "propagateToChildren": true}}
- propose_completion: {{"taskId": "<id>", "taskRevision": <revision>, "runId": "<run id>", "artifacts": [], "summary": "<what was done>"}}
- ask_user: {{"question": "<question>", "reason": "<why>", "options": [], "relatedTaskIds": []}}

Current plan state:
{state}

Message:
{message}"#
    )
}
