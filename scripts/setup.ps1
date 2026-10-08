# Prepara el entorno de desarrollo en Windows: instala lo que falte con winget.
# Uso (desde la raíz del repo): powershell -ExecutionPolicy Bypass -File scripts\setup.ps1
$ErrorActionPreference = "Stop"

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    throw "No se encuentra winget. Actualiza 'Instalador de aplicación' desde Microsoft Store y reintenta."
}

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = "$machine;$user;$env:USERPROFILE\.cargo\bin"
}

function Install-WithWinget([string]$id, [string[]]$extra = @()) {
    Write-Host "Instalando $id ..."
    winget install --id $id --exact --silent --accept-source-agreements --accept-package-agreements @extra
}

function Test-BuildTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -products * -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}

Update-SessionPath

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Install-WithWinget "OpenJS.NodeJS.LTS"
}

if (-not (Test-BuildTools)) {
    Install-WithWinget "Microsoft.VisualStudio.2022.BuildTools" @(
        "--override",
        "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    )
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Install-WithWinget "Rustlang.Rustup"
    Update-SessionPath
    rustup default stable-x86_64-pc-windows-msvc
}

# WebView2 viene con Windows 10/11 actualizados; se instala solo si falta.
$webview2Key = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
if (-not (Test-Path $webview2Key)) {
    Install-WithWinget "Microsoft.EdgeWebView2Runtime"
}

Update-SessionPath
npm install

Write-Host ""
Write-Host "Versiones instaladas:"
node --version
npm --version
cargo --version
Write-Host ""
Write-Host "Listo. Si cargo o node no se reconocen, cierra y abre PowerShell. Después: npm run tauri dev"
