//! Reason Network Fork Local RPC 클라이언트 (Spec §12.1). 소유: PH0-RN.
//!
//! RPC 메시지 타입은 Core와 RN의 경계 계약이므로 `telos_schema::rn`에 정의하고 여기서 재노출합니다 (§3.4).
//! 전송·연결·재시도 등 클라이언트 구현은 PH0-RN이 추가합니다. G0에서는 계약 재노출만 등록되어 있습니다.

pub use telos_schema::rn::*;
