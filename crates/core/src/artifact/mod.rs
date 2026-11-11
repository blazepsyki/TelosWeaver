//! Artifact 저장소 (Spec §10, PH1-ARTIFACT). H0 공개 인터페이스이며 구현은 PH1-ARTIFACT가 합니다.
//!
//! 내용은 SHA-256 주소(`artifact://sha256/<hex>`)로 저장하고 Version 기록은 따로 둡니다(H0 D2).
//! 기본 한도는 `DEFAULT_ARTIFACT_MAX_BYTES`이며, 참조(`ArtifactReference`)가 남은 내용은 정리하지 않습니다.

use std::path::Path;

use telos_schema::{ArtifactHolder, ArtifactKind, ArtifactVersion, ArtifactVersionRef};

/// 새 Version의 출처 정보입니다. `checksum`·`storage_uri`·`size_bytes`는 저장소가 채웁니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArtifact {
    /// 기존 Artifact의 새 Version이면 그 ID
    pub artifact_id: Option<String>,
    pub task_id: String,
    pub task_revision: u32,
    pub run_id: String,
    pub output_name: String,
    pub kind: ArtifactKind,
    pub content_type: String,
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    TooLarge {
        size_bytes: u64,
        limit: u64,
    },
    NotFound {
        target: String,
    },
    /// 저장 내용의 Checksum이 기록과 다름
    Corrupt {
        target: String,
    },
    Io {
        message: String,
    },
}

pub trait ArtifactStore: Send + Sync {
    /// 파일 내용을 복사해 불변 Version을 만듭니다. Workspace 경로는 이후 바뀌어도 영향이 없습니다.
    fn put_file(&self, meta: NewArtifact, path: &Path) -> Result<ArtifactVersion, ArtifactError>;
    fn put_bytes(&self, meta: NewArtifact, bytes: &[u8]) -> Result<ArtifactVersion, ArtifactError>;
    /// 일부 읽기. 읽을 때 Checksum을 확인합니다 (API `artifacts.read`는 `MAX_ARTIFACT_READ_BYTES` 이하).
    fn read(
        &self,
        target: &ArtifactVersionRef,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, ArtifactError>;
    fn add_reference(
        &self,
        target: &ArtifactVersionRef,
        holder: ArtifactHolder,
    ) -> Result<(), ArtifactError>;
    /// 참조 없는 저장 내용을 정리하고 지운 개수를 돌려줍니다.
    fn collect_garbage(&self) -> Result<u64, ArtifactError>;
}
