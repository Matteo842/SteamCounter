# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Matteo842
# Regenerate before building the standalone Windows executable.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $metadataText = cargo metadata --format-version 1 --locked --offline --features gui --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Could not read dependency metadata' }
    $metadata = $metadataText | ConvertFrom-Json
    $tree = cargo tree --features gui --locked --offline --target x86_64-pc-windows-msvc --edges normal,build --prefix none --format '{p}'
    if ($LASTEXITCODE -ne 0) { throw 'Could not list dependencies' }
    $ids = $tree | ForEach-Object { if ($_ -match '^([^ ]+) v([^ ]+)') { "$($Matches[1])@$($Matches[2])" } } | Sort-Object -Unique
    $packages = $metadata.packages | Where-Object { $ids -contains "$($_.name)@$($_.version)" -and $_.name -ne 'steamcounter' } | Sort-Object name,version
    $noticeRoot = Join-Path $repoRoot 'docs/third-party'
    $sourceRoot = Join-Path $noticeRoot 'sources'
    New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null
    $notices = [Text.StringBuilder]::new()
    [void]$notices.AppendLine('SteamCounter - third-party software and font notices')
    [void]$notices.AppendLine('Exact Windows runtime and build dependencies from Cargo.lock. Libraries, fonts and other assets retain their original licenses. Texts are included below and identical texts are shared by reference number. These notices are embedded in the standalone executable and available in Settings > View licenses.')
    [void]$notices.AppendLine('SteamCounter source is GPL-3.0-or-later. The MPL-2.0 components are also made available under GPL-3.0-or-later as part of this combined work, under MPL section 3.3; their original MPL notices remain intact. The matching source and source-download index are in the same version tag at https://github.com/Matteo842/SteamCounter. See docs/third-party/SOURCES.md.')
    [void]$notices.AppendLine('The app is not affiliated with Valve, SteamCharts or SteamDB. These licenses do not license their data or trademarks.')
    $sources = [Text.StringBuilder]::new()
    [void]$sources.AppendLine('# Corresponding dependency sources')
    [void]$sources.AppendLine("`nThese exact, unmodified crate archives contain the runtime and build dependency sources for the Windows GUI release. Cargo.lock records their versions and SHA-256 checksums; Cargo downloads and verifies them automatically when building. Download SteamCounter itself from the matching release tag and follow the build instructions in the root README. Rust and the platform compiler/SDK are required build tools.")
    [void]$sources.AppendLine("`nMPL-2.0 archives are also mirrored in this repository's sources/ folder and additionally available under GPL-3.0-or-later as part of the combined work, under MPL section 3.3. Their original notices and MPL rights are preserved. Shared egui license texts are in this folder. Font sources and licenses are included in epaint_default_fonts.")
    [void]$sources.AppendLine("`n| Component | License expression | Exact source |")
    [void]$sources.AppendLine('| --- | --- | --- |')
    $licenseIds = @{}
    $licenseBodies = [Collections.Generic.List[string]]::new()
    function Add-LicenseText([string]$body) {
        $normalized = $body.Replace("`r`n", "`n").Trim()
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($normalized)))
        if (-not $licenseIds.ContainsKey($hash)) {
            $licenseBodies.Add($normalized)
            $licenseIds[$hash] = $licenseBodies.Count
        }
        return $licenseIds[$hash]
    }
    $sharedIds = foreach ($file in 'egui-LICENSE-MIT.txt','egui-LICENSE-APACHE.txt') {
        Add-LicenseText ([IO.File]::ReadAllText((Join-Path $noticeRoot $file)))
    }
    $boostPackage = $packages | Where-Object name -EQ 'error-code' | Select-Object -First 1
    $boostFile = rg --files (Split-Path -Parent $boostPackage.manifest_path) --iglob '*license*' | Select-Object -First 1
    if (-not $boostFile) { throw 'Missing Boost license text' }
    $boostId = Add-LicenseText ([IO.File]::ReadAllText($boostFile))
    $mplPackage = $packages | Where-Object name -EQ 'cssparser' | Select-Object -First 1
    $mplId = Add-LicenseText ([IO.File]::ReadAllText((Join-Path (Split-Path -Parent $mplPackage.manifest_path) 'LICENSE')))
    foreach ($package in $packages) {
        $crateRoot = Split-Path -Parent $package.manifest_path
        $url = "https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download"
        [void]$notices.AppendLine("`n========================================`n$($package.name) $($package.version)`nLicense: $($package.license)`nAuthors: $($package.authors -join ', ')`nRepository: $($package.repository)`nSource: $url")
        [void]$sources.AppendLine("| $($package.name) $($package.version) | $($package.license) | [Download]($url) |")
        $files = @(rg --files --hidden $crateRoot --iglob '*license*' --iglob '*licence*' --iglob '*copying*' --iglob '*notice*' --iglob 'OFL.txt' --iglob 'UFL.txt' --iglob 'Hack-Regular.txt' | Sort-Object)
        if ($files.Count -eq 0) {
            if ($package.name -in @('ecolor','eframe','egui','egui-winit','egui_glow','emath','epaint')) {
                [void]$notices.AppendLine("License texts: $($sharedIds -join ', ') (egui workspace)")
            } elseif ($package.name -eq 'clipboard-win') {
                [void]$notices.AppendLine("License text: $boostId (Boost Software License 1.0)")
            } elseif ($package.name -in @('fxhash','gl_generator','khronos_api','mac','match_token')) {
                [void]$notices.AppendLine("License text: $($sharedIds[1]) (Apache 2.0; selected where dual-licensed)")
                $copyrightLines = @(rg --no-heading --no-filename '^//.*[Cc]opyright' $crateRoot -g '*.rs' | Sort-Object -Unique)
                foreach ($line in $copyrightLines) { [void]$notices.AppendLine($line) }
            } elseif ($package.name -eq 'selectors') {
                [void]$notices.AppendLine("License text: $mplId (Mozilla Public License 2.0)")
            } else { throw "No license text for $($package.name); review before releasing" }
        }
        foreach ($file in $files) {
            $id = Add-LicenseText ([IO.File]::ReadAllText($file))
            [void]$notices.AppendLine("License text $id : $([IO.Path]::GetRelativePath($crateRoot, $file))")
        }
        if ($package.name -eq 'epaint_default_fonts') {
            [void]$notices.AppendLine("Rust code license texts: $($sharedIds -join ', ') (egui workspace). Bundled fonts retain the font licenses above.")
        }
        if ($package.license -match 'MPL-2.0') {
            $rootInfo = Get-Item -LiteralPath $crateRoot
            $registry = $rootInfo.Parent.Parent.Parent.FullName
            $archiveName = "$($package.name)-$($package.version).crate"
            $archive = Join-Path $registry "cache/$($rootInfo.Parent.Name)/$archiveName"
            Copy-Item -LiteralPath $archive -Destination $sourceRoot
        }
    }
    for ($i = 0; $i -lt $licenseBodies.Count; $i++) {
        [void]$notices.AppendLine("`n========================================`nLICENSE TEXT $($i + 1)`n========================================`n$($licenseBodies[$i])")
    }
    [IO.File]::WriteAllText((Join-Path $noticeRoot 'THIRD_PARTY_NOTICES.txt'), $notices.ToString(), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $noticeRoot 'SOURCES.md'), $sources.ToString(), [Text.UTF8Encoding]::new($false))
    [PSCustomObject]@{ Dependencies=$packages.Count; UniqueLicenseTexts=$licenseBodies.Count; NoticeBytes=(Get-Item (Join-Path $noticeRoot 'THIRD_PARTY_NOTICES.txt')).Length }
} finally { Pop-Location }
