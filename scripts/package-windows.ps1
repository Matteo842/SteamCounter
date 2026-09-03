# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Matteo842
# Build and package the single portable Windows GUI executable.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot 'generate-notices.ps1') | Out-Null
    cargo build --release --locked --offline --features gui --bins
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $metadataText = cargo metadata --format-version 1 --locked --offline --no-deps
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the package version' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -EQ 'steamcounter').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version' }
    $output = Join-Path $repoRoot "target/packages/SteamCounter-$version-windows-x64.exe"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $output) | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'target/release/steamcounter-gui.exe') -Destination $output
    # The digest goes in the release notes; the executable is the only uploaded asset.
    [PSCustomObject]@{ Executable=$output; Bytes=(Get-Item -LiteralPath $output).Length; SHA256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant() }
} finally { Pop-Location }
