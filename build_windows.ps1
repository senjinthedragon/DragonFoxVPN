<#
build_windows.ps1 - DragonFoxVPN: Windows build script
Copyright (c) 2026 Senjin the Dragon.
https://github.com/senjinthedragon/DragonFoxVPN
Licensed under the MIT License.
See LICENSE for full license information.

Automates the Windows release build using Cargo.
The output binary is placed in target\release\DragonFoxVPN.exe.
Requires Rust (https://rustup.rs) and the MSVC toolchain.
#>
# Prerequisites: Rust stable toolchain (rustup), MSVC build tools

$ErrorActionPreference = "Stop"

Write-Host "Building DragonFoxVPN (Rust release)..."
cargo build --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "Build successful! Executable is in target\release\DragonFoxVPN.exe"
}
else {
    Write-Host "Build failed!"
    exit 1
}
