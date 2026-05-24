param(
    [string]$Version = "0.1.0",
    [string]$TargetName = "windows-x86_64",
    [switch]$SkipChecks
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$DistDir = Join-Path $RepoRoot "dist"
$PackageName = "keyzen-v$Version-$TargetName"
$PackageDir = Join-Path $DistDir $PackageName
$ExamplesDir = Join-Path $PackageDir "examples"
$ZipPath = Join-Path $DistDir "$PackageName.zip"
$ZipHashPath = Join-Path $DistDir "$PackageName.zip.sha256"
$ExeHashPath = Join-Path $DistDir "keyzen.exe.sha256"
$ReleaseNotesPath = Join-Path $DistDir "RELEASE_NOTES-v$Version.md"
$ReleaseExe = Join-Path $RepoRoot "target\release\keyzen.exe"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Command
    )

    Write-Host "==> $Name"
    & $Command
}

function Write-Sha256File {
    param(
        [string]$InputPath,
        [string]$DisplayPath,
        [string]$OutputPath
    )

    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $InputPath
    "$($hash.Hash)  $DisplayPath" | Set-Content -NoNewline -Encoding ASCII -LiteralPath $OutputPath
}

Set-Location $RepoRoot

if (-not $SkipChecks) {
    Invoke-Step "cargo fmt --all --check" {
        cargo fmt --all --check
    }

    Invoke-Step "cargo test --workspace" {
        cargo test --workspace
    }
}

Invoke-Step "cargo build --release -p keyzen" {
    cargo build --release -p keyzen
}

if (-not (Test-Path -LiteralPath $ReleaseExe)) {
    throw "Release executable was not found: $ReleaseExe"
}

Invoke-Step "prepare dist directory" {
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

    foreach ($path in @($PackageDir, $ZipPath, $ZipHashPath, $ExeHashPath, $ReleaseNotesPath)) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }

    New-Item -ItemType Directory -Force -Path $ExamplesDir | Out-Null
}

Invoke-Step "copy package files" {
    Copy-Item -LiteralPath $ReleaseExe -Destination (Join-Path $PackageDir "keyzen.exe")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination (Join-Path $PackageDir "README.md")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "AGENTS.md") -Destination (Join-Path $PackageDir "AGENTS.md")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "examples\config.toml") -Destination (Join-Path $ExamplesDir "config.toml")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "examples\keyzen.toml") -Destination (Join-Path $ExamplesDir "keyzen.toml")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "examples\vim.toml") -Destination (Join-Path $ExamplesDir "vim.toml")
}

Invoke-Step "write release notes" {
    @"
# KeyZen v$Version

KeyZen v$Version is a local MVP package for Windows.

## Highlights

- Windows low-level keyboard hook backend with no driver installation.
- Layer-based keymap engine with single key output, modifier chords, transparent keys, no-op keys, hold layers, and layer switching.
- Tray controls for pause/resume, config reload, config folder opening, keymap file selection, startup toggle, and exit.
- App config is created in the OS config directory, for example `%APPDATA%\KeyZen\config.toml` on Windows.
- The default keymap is embedded in the executable and does not depend on repository `examples/` files.

## Package

This is an unsigned portable Windows build. Windows may show a warning when launching the executable.

The package includes:

- `keyzen.exe`
- `README.md`
- `AGENTS.md`
- `examples/config.toml`
- `examples/keyzen.toml`
- `examples/vim.toml`

## Known Limits

The MVP uses `LowLevelKeyboardHook`, so some OS-reserved shortcuts such as `Win+L` may run before KeyZen can suppress them.

KeyZen v$Version intentionally does not include text macros, shell commands, delayed automation, mouse automation, Unicode text insertion, tap-hold, tap-dance, or one-shot keys.
"@ | Set-Content -Encoding UTF8 -LiteralPath $ReleaseNotesPath
}

Invoke-Step "create zip package" {
    Compress-Archive -LiteralPath $PackageDir -DestinationPath $ZipPath -Force
}

Invoke-Step "write SHA-256 checksums" {
    Write-Sha256File -InputPath $ZipPath -DisplayPath "dist/$PackageName.zip" -OutputPath $ZipHashPath
    Write-Sha256File -InputPath (Join-Path $PackageDir "keyzen.exe") -DisplayPath "dist/$PackageName/keyzen.exe" -OutputPath $ExeHashPath
}

Invoke-Step "validate package contents" {
    $requiredFiles = @(
        (Join-Path $PackageDir "keyzen.exe"),
        (Join-Path $PackageDir "README.md"),
        (Join-Path $PackageDir "AGENTS.md"),
        (Join-Path $ExamplesDir "config.toml"),
        (Join-Path $ExamplesDir "keyzen.toml"),
        (Join-Path $ExamplesDir "vim.toml"),
        $ZipPath,
        $ZipHashPath,
        $ExeHashPath,
        $ReleaseNotesPath
    )

    foreach ($path in $requiredFiles) {
        if (-not (Test-Path -LiteralPath $path)) {
            throw "Missing package file: $path"
        }
    }

    $zipEntries = tar -tf $ZipPath
    foreach ($entry in @(
        "$PackageName/keyzen.exe",
        "$PackageName/README.md",
        "$PackageName/AGENTS.md",
        "$PackageName/examples/config.toml",
        "$PackageName/examples/keyzen.toml",
        "$PackageName/examples/vim.toml"
    )) {
        if ($zipEntries -notcontains $entry) {
            throw "Missing zip entry: $entry"
        }
    }
}

Write-Host ""
Write-Host "Package complete:"
Write-Host "  $ZipPath"
Write-Host "  $ZipHashPath"
Write-Host "  $ExeHashPath"
Write-Host "  $ReleaseNotesPath"
