# Build freedrive_thumb.dll and copy into src-tauri/resources for NSIS bundling.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root "src-tauri\crates\freedrive_thumb\Cargo.toml"
$outDir = Join-Path $root "src-tauri\resources"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

Write-Host "Building freedrive_thumb (release)..."
cargo build --release --manifest-path $manifest
$dll = Join-Path $root "src-tauri\target\release\freedrive_thumb.dll"
if (-not (Test-Path $dll)) {
  # Workspace may place artifact under crates target when built in isolation
  $alt = Join-Path $root "src-tauri\crates\freedrive_thumb\target\release\freedrive_thumb.dll"
  if (Test-Path $alt) { $dll = $alt }
}
if (-not (Test-Path $dll)) {
  throw "freedrive_thumb.dll not found after build"
}
Copy-Item -Force $dll (Join-Path $outDir "freedrive_thumb.dll")
Write-Host "Copied to src-tauri\resources\freedrive_thumb.dll"
