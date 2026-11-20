# Publishes ISObubble to https://github.com/robwalsh147/ISObubble
#  1. uploads the source code (branch "main")
#  2. creates a Release with ISObubble.exe attached, which installed copies use for updates.
# Double-click "Publish to GitHub.bat" to run it. Git asks you to sign in to GitHub the first time.

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$repo = 'robwalsh147/ISObubble'
$remote = "https://github.com/$repo.git"

function Step($t) { Write-Host "`n== $t" -ForegroundColor Magenta }

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host 'Git is not installed. Get it from https://git-scm.com/download/win and run this again.' -ForegroundColor Red
    Read-Host 'Press Enter to close'; exit 1
}

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$tag = "v$version"
$exe = Join-Path $PSScriptRoot 'dist\ISObubble.exe'

Step "Preparing ISObubble $tag"
if (-not (Test-Path $exe)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        cargo build --release
        New-Item -ItemType Directory -Force dist | Out-Null
        Copy-Item target\release\isobubble.exe $exe -Force
    } else {
        Write-Host 'dist\ISObubble.exe is missing and Rust is not installed to build it.' -ForegroundColor Red
        Read-Host 'Press Enter to close'; exit 1
    }
}

Step 'Uploading the source code'
if (-not (Test-Path .git)) { git init -b main | Out-Null }
git add -A
git diff --cached --quiet
if ($LASTEXITCODE -ne 0) { git commit -m "ISObubble $tag" | Out-Null }
$hasOrigin = (git remote) -contains 'origin'
if ($hasOrigin) { git remote set-url origin $remote } else { git remote add origin $remote }
git fetch origin 2>$null
git rev-parse --verify --quiet origin/main | Out-Null
if ($LASTEXITCODE -eq 0) {
    # Keep anything already on GitHub (e.g. a README made on the website); our files win on conflicts.
    git pull origin main --allow-unrelated-histories --no-edit -X ours
}
git push -u origin main
if ($LASTEXITCODE -ne 0) { Write-Host 'Upload failed — check you are signed in to GitHub as robwalsh147.' -ForegroundColor Red; Read-Host 'Press Enter to close'; exit 1 }

Step "Creating the $tag release with ISObubble.exe"
$notes = "ISObubble $tag`n`nDownload ISObubble.exe, run it, then use Settings & updates > Install ISObubble on this PC. Installed copies update themselves from this page."
if (Get-Command gh -ErrorAction SilentlyContinue) {
    gh release create $tag $exe --repo $repo --title "ISObubble $tag" --notes $notes
    if ($LASTEXITCODE -eq 0) { Write-Host "`nDone! https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green; Read-Host 'Press Enter to close'; exit 0 }
}
# No GitHub CLI: open the "new release" page and the folder with ISObubble.exe to drag in.
Start-Process "https://github.com/$repo/releases/new?tag=$tag&title=ISObubble%20$tag"
Start-Process explorer.exe "/select,`"$exe`""
Write-Host "`nThe source is uploaded. To finish the release:" -ForegroundColor Green
Write-Host "  1. In the browser page that opened, drag ISObubble.exe (shown in the Explorer window) into the box at the bottom."
Write-Host "  2. Press 'Publish release'."
Read-Host "`nPress Enter to close"
