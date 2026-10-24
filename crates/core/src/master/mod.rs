//! Master Controller — 마스터 호출·결정 회수·집행 (Spec §4). 소유: PH0-MASTER.
//!
//! Phase 0 최소 경로(Spec §19.1): 마스터 후보(배정표 §6: 임시 Claude)를 제어용 Task(`role: master`)로 실행하고,
//! 최종 응답의 JSON 결정 하나를 회수해 검증(`crate::policy::validate`)한 뒤 허용된 결정을 집행하고 결과를 기록합니다.
//!
//! - 마스터 출력은 신뢰하지 않는 입력입니다. [`parse_master_output`]은 정해진 형태 외의 필드를 거부합니다.
//! - 결정의 ID·Plan·Epoch·Policy Revision·마스터 Run은 Core가 채웁니다. 마스터는 `kind`·`payload`·`rationale`만 정합니다.
//! - 같은 멱등 키·같은 내용은 처음 결과를 `replayed`로 돌려주고 다시 집행하지 않습니다. 같은 키·다른 내용은 거부합니다.
//! - 집행 결과는 다음 마스터 Turn에 Resume으로 전달합니다(결과 왕복).
//! - 상태는 메모리에만 둡니다. 영속화·재시작 복구는 P0 범위입니다.

mod controller;
mod output;

pub use controller::{MasterController, MasterSettings, MasterTurn, PlanTaskView, RunExecutor};
pub use output::{MasterOutput, master_prompt, parse_master_output};
