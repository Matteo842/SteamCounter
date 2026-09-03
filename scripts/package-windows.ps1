# Run after: cargo build --release --locked --features gui --bins
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $metadataText = cargo metadata --format-version 1 --locked --offline --features gui --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Could not read dependency metadata' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -EQ 'steamcounter').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version' }
    $packageName = "SteamCounter-v$version-windows-x64"
    $packageRoot = Join-Path $repoRoot "target/packages/$packageName"
    $packageBase = Join-Path $repoRoot 'target/packages'
    New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null

    foreach ($binary in 'steamcounter.exe', 'steamcounter-gui.exe') {
        Copy-Item -LiteralPath (Join-Path $repoRoot "target/release/$binary") -Destination $packageRoot
    }
    foreach ($file in 'README.md', 'docs/DATA_SOURCES.md', 'docs/previews/home.png', 'docs/previews/dashboard.png') {
        $destination = Join-Path $packageRoot $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath (Join-Path $repoRoot $file) -Destination $destination
    }

    $tree = cargo tree --features gui --locked --offline --target x86_64-pc-windows-msvc --edges normal --prefix none --format '{p}'
    if ($LASTEXITCODE -ne 0) { throw 'Could not list runtime dependencies' }
    $ids = $tree | ForEach-Object { if ($_ -match '^([^ ]+) v([^ ]+)') { "$($Matches[1])@$($Matches[2])" } } | Sort-Object -Unique
    $packages = $metadata.packages | Where-Object { $ids -contains "$($_.name)@$($_.version)" -and $_.name -ne 'steamcounter' } | Sort-Object name,version
    $notices = [Text.StringBuilder]::new()
    [void]$notices.AppendLine('SteamCounter - third-party software and font notices')
    [void]$notices.AppendLine('License identifiers, authors and source links are listed below. License texts from the packaged dependencies follow each entry. Shared egui MIT/Apache texts are included at the end. Some dual-licensed crates refer to these same standard Apache 2.0 terms. Unmodified MPL-covered crate sources are included in third_party_sources.')
    [void]$notices.AppendLine('The app is not affiliated with Valve, SteamCharts or SteamDB. These software licenses do not license their trademarks or data.')
    $missing = [Collections.Generic.List[string]]::new()
    foreach ($package in $packages) {
        $crateRoot = Split-Path -Parent $package.manifest_path
        [void]$notices.AppendLine("`n========================================")
        [void]$notices.AppendLine("$($package.name) $($package.version)")
        [void]$notices.AppendLine("License: $($package.license)")
        [void]$notices.AppendLine("Authors: $($package.authors -join ', ')")
        [void]$notices.AppendLine("Repository: $($package.repository)")
        [void]$notices.AppendLine("Source: https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download")
        $files = @(rg --files --hidden $crateRoot --iglob '*license*' --iglob '*licence*' --iglob '*copying*' --iglob '*notice*' --iglob 'OFL.txt' --iglob 'UFL.txt' --iglob 'Hack-Regular.txt')
        if ($files.Count -eq 0) { $missing.Add($package.name) }
        foreach ($file in $files) {
            [void]$notices.AppendLine("`n--- $([IO.Path]::GetRelativePath($crateRoot, $file)) ---")
            [void]$notices.AppendLine([IO.File]::ReadAllText($file))
        }
        if ($package.license -match 'MPL-2.0') {
            $rootInfo = Get-Item -LiteralPath $crateRoot
            $registry = $rootInfo.Parent.Parent.Parent.FullName
            $archive = Join-Path $registry "cache/$($rootInfo.Parent.Name)/$($package.name)-$($package.version).crate"
            $sourceDir = Join-Path $packageRoot 'third_party_sources'
            New-Item -ItemType Directory -Force -Path $sourceDir | Out-Null
            Copy-Item -LiteralPath $archive -Destination $sourceDir
        }
    }
    foreach ($file in 'egui-LICENSE-MIT.txt','egui-LICENSE-APACHE.txt') {
        [void]$notices.AppendLine("`n--- Shared egui 0.29.1 terms: $file ---")
        [void]$notices.AppendLine([IO.File]::ReadAllText((Join-Path $repoRoot "docs/third-party/$file")))
    }
    # clipboard-win omits the Boost text from its crate; error-code includes the same standard license.
    $boostPackage = $packages | Where-Object name -EQ 'error-code' | Select-Object -First 1
    $boostFiles = @(rg --files (Split-Path -Parent $boostPackage.manifest_path) --iglob '*license*')
    if ($boostFiles.Count -eq 0) { throw 'Missing Boost Software License text' }
    [void]$notices.AppendLine("`n--- clipboard-win: Boost Software License 1.0 ---")
    foreach ($file in $boostFiles) { [void]$notices.AppendLine([IO.File]::ReadAllText($file)) }
    [IO.File]::WriteAllText((Join-Path $packageRoot 'THIRD_PARTY_NOTICES.txt'), $notices.ToString(), [Text.UTF8Encoding]::new($false))

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipPath = Join-Path $packageBase "$packageName.zip"
    # This file is a generated artifact, never a source folder.
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath }
    [IO.Compression.ZipFile]::CreateFromDirectory($packageRoot, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $false)
    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $packageBase "$packageName.sha256"), "$hash  $packageName.zip`n", [Text.UTF8Encoding]::new($false))
    [PSCustomObject]@{ Package=$zipPath; Bytes=(Get-Item -LiteralPath $zipPath).Length; Dependencies=$packages.Count; SharedLicenseTexts=($missing -join ', ') }
} finally { Pop-Location }
