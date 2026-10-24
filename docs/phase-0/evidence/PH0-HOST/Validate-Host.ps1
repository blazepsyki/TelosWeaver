param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path,
    [string]$ValidationRoot = (Join-Path ([IO.Path]::GetTempPath()) ('telos-host-validation-' + [guid]::NewGuid().ToString('N'))),
    [switch]$CheckLinux,
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$sourceRoot = (Resolve-Path -LiteralPath $RepoRoot).Path.Replace('\','/')
$validationPath = [IO.Path]::GetFullPath($ValidationRoot)
if (Test-Path -LiteralPath $validationPath) { throw '검증 디렉터리가 이미 있습니다. 새 경로를 지정하십시오.' }
New-Item -ItemType Directory -Path $validationPath | Out-Null
foreach ($name in @('adapters','core')) { New-Item -ItemType Directory -Path (Join-Path $validationPath $name) | Out-Null }

# Keep validation dependencies outside the repository until REQ-011 is integrated.
$workspace = @"
[workspace]
resolver = "3"
members = ["adapters", "core"]
[workspace.dependencies]
async-trait = "0.1.92"
futures = "0.3.34"
serde = {version="1.0.229",features=["derive"]}
serde_json = "1.0.154"
sha2 = "0.11.0"
semver = "1.0.28"
getrandom = "0.4.3"
libc = "0.2.191"
tokio = {version="1.54.1",features=["net","io-util","sync","rt-multi-thread","time","macros","signal"]}
windows-sys = {version="0.61.2",features=["Win32_Foundation","Win32_Security","Win32_Security_Authorization","Win32_System_Threading","Win32_System_JobObjects","Win32_System_Pipes","Win32_Storage_FileSystem"]}
telos-schema = {path="$sourceRoot/crates/schema",features=["fixtures"]}
"@
$adapters = @"
[package]
name = "telos-adapters"
version = "0.0.0"
edition = "2024"
[lib]
path = "$sourceRoot/crates/adapters/src/lib.rs"
[dependencies]
async-trait.workspace = true
futures.workspace = true
serde_json.workspace = true
sha2.workspace = true
semver.workspace = true
tokio.workspace = true
telos-schema.workspace = true
[target.'cfg(windows)'.dependencies]
windows-sys.workspace = true
[target.'cfg(unix)'.dependencies]
libc.workspace = true
[[test]]
name = "runtime_native"
path = "$sourceRoot/crates/adapters/tests/runtime_native.rs"
[[test]]
name = "discovery_extensions"
path = "$sourceRoot/crates/adapters/tests/discovery_extensions.rs"
[[test]]
name = "discovery_resolution"
path = "$sourceRoot/crates/adapters/tests/discovery_resolution.rs"
"@
$core = @"
[package]
name = "telos-core"
version = "0.0.0"
edition = "2024"
[lib]
path = "$sourceRoot/crates/core/src/lib.rs"
[[bin]]
name = "telos-weaver-host-validation"
path = "$sourceRoot/docs/phase-0/evidence/PH0-HOST/daemon_entrypoint.rs"
[dependencies]
telos-adapters = {path="../adapters"}
telos-schema.workspace = true
telos-rn-client = {path="$sourceRoot/crates/rn-client"}
tokio.workspace = true
futures.workspace = true
serde.workspace = true
serde_json.workspace = true
getrandom.workspace = true
async-trait.workspace = true
[target.'cfg(windows)'.dependencies]
windows-sys.workspace = true
[target.'cfg(unix)'.dependencies]
libc.workspace = true
[[test]]
name = "host_execution"
path = "$sourceRoot/crates/core/tests/host_execution.rs"
[[test]]
name = "host_api"
path = "$sourceRoot/crates/core/tests/host_api.rs"
"@
Set-Content -LiteralPath (Join-Path $validationPath 'Cargo.toml') -Value $workspace -Encoding utf8NoBOM
Set-Content -LiteralPath (Join-Path $validationPath 'adapters/Cargo.toml') -Value $adapters -Encoding utf8NoBOM
Set-Content -LiteralPath (Join-Path $validationPath 'core/Cargo.toml') -Value $core -Encoding utf8NoBOM
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'VALIDATION-Cargo.lock') -Destination (Join-Path $validationPath 'Cargo.lock')
$manifest = Join-Path $validationPath 'Cargo.toml'
$cargoOptions = @('--manifest-path', $manifest, '--locked')
if ($Offline) { $cargoOptions += '--offline' }
function Invoke-HostCargo([string[]]$CargoArguments) {
    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "검증 명령이 실패했습니다. 종료 코드: $LASTEXITCODE" }
}
Invoke-HostCargo (@('test') + $cargoOptions + @('--workspace','--','--test-threads=1'))
Invoke-HostCargo (@('clippy') + $cargoOptions + @('--workspace','--all-targets','--','-D','warnings'))
if ($CheckLinux) {
    Invoke-HostCargo (@('clippy') + $cargoOptions + @('--workspace','--all-targets','--target','x86_64-unknown-linux-gnu','--','-D','warnings'))
}
Write-Host "검증 완료. 독립 Manifest 위치: $manifest"
Write-Host '공식 Workspace 통합이나 Linux 실행 검증의 완료를 뜻하지 않습니다.'
