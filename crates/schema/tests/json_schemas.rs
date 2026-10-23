//! 저장소의 `schemas/*.schema.json`이 Rust 원본과 일치하는지 확인합니다 (Spec §3.4).
//! 타입을 바꾼 뒤에는 `UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas`로 갱신합니다.

use std::{fs, path::PathBuf};

#[test]
fn committed_json_schemas_are_up_to_date() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let update = std::env::var_os("UPDATE_SCHEMAS").is_some();
    if update {
        fs::create_dir_all(&dir).unwrap();
    }

    let mut stale = Vec::new();
    for (name, schema) in telos_schema::json_schemas() {
        let path = dir.join(format!("{name}.schema.json"));
        let expected = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        if update {
            fs::write(&path, expected).unwrap();
        } else if fs::read_to_string(&path)
            .map(|committed| committed.replace("\r\n", "\n"))
            .ok()
            .as_deref()
            != Some(expected.as_str())
        {
            // core.autocrlf 환경에서 CRLF로 체크아웃되어도 같은 내용으로 봅니다.
            stale.push(path.display().to_string());
        }
    }

    assert!(
        stale.is_empty(),
        "JSON Schema가 Rust 타입과 다릅니다. UPDATE_SCHEMAS=1로 다시 생성하세요: {stale:?}"
    );
}
