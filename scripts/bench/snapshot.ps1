#requires -Version 5.1
<#
.SYNOPSIS
    Fly IDE Phase 0 benchmark: cold-start marks + process-tree memory.
.DESCRIPTION
    Launches Fly IDE, waits for the WebView2 child process, samples memory,
    prints startup marks from %LOCALAPPDATA%\FlyIDE\logs\startup.jsonl, then exits.
    NOTE: keep this file ASCII-only (Windows PowerShell 5.1 reads scripts as ANSI).
.PARAMETER Exe
    Path to fly-ide.exe. Default: workspace debug build.
.PARAMETER Seconds
    Memory sampling duration in seconds. Default 10.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench\snapshot.ps1 -Exe target\release\fly-ide.exe
#>
[CmdletBinding()]
param(
    [string]$Exe = "apps\desktop\src-tauri\target\debug\fly-ide.exe",
    [int]$Seconds = 10
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$exePath = if ([System.IO.Path]::IsPathRooted($Exe)) { $Exe } else { Join-Path $repoRoot $Exe }
if (-not (Test-Path $exePath)) {
    Write-Error "Executable not found: $exePath`nBuild it first (pnpm tauri dev / cargo build --release)."
    exit 1
}

$logFile = Join-Path $env:LOCALAPPDATA "FlyIDE\logs\startup.jsonl"
if (Test-Path $logFile) { Remove-Item $logFile -Force }

Write-Host "== Fly IDE benchmark ==" -ForegroundColor Cyan
Write-Host "Executable: $exePath"

$proc = Start-Process -FilePath $exePath -PassThru
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$windowReady = $false
while ($sw.Elapsed.TotalSeconds -lt 30) {
    Start-Sleep -Milliseconds 100
    if ($proc.HasExited) { Write-Error "Process exited early, code: $($proc.ExitCode)"; exit 1 }
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($proc.Id)" -ErrorAction SilentlyContinue
    if ($children) { $windowReady = $true; break }
}
if (-not $windowReady) { Write-Warning "No WebView2 child process within 30s" }

# WebView2 会先于 Tauri setup 完成前 spawn 辅助进程（如 crashpad），
# 这里等到出现 tauri.setup.end 标记（或最多 5 秒）再读取，保证拿到完整启动链。
$deadline = (Get-Date).AddSeconds(5)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 100
    if ((Test-Path $logFile) -and (Select-String -Path $logFile -Pattern "tauri.setup.end" -Quiet)) { break }
}
Start-Sleep -Milliseconds 200

Write-Host ""
Write-Host "-- Cold start marks (ms since process start) --" -ForegroundColor Cyan
if (Test-Path $logFile) {
    Get-Content $logFile | ForEach-Object {
        try {
            $entry = $_ | ConvertFrom-Json
            "{0,-24} {1,9:N1} ms" -f $entry.mark, $entry.ms
        } catch {
            $_
        }
    }
} else {
    Write-Warning "No startup marks found: $logFile"
}

Write-Host ""
Write-Host "-- Process tree memory (${Seconds}s) --" -ForegroundColor Cyan
$samples = @()
for ($i = 0; $i -lt $Seconds; $i++) {
    Start-Sleep -Seconds 1
    if ($proc.HasExited) { break }
    $tree = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($proc.Id)" -ErrorAction SilentlyContinue
    $all = @($proc) + $tree
    $ws = 0L
    $priv = 0L
    foreach ($p in $all) {
        $ws += $p.WorkingSetSize
        $priv += $p.PrivatePageCount
    }
    $samples += [pscustomobject]@{
        WorkingSetMB = [math]::Round($ws / 1MB, 1)
        PrivateMB    = [math]::Round($priv / 1MB, 1)
    }
}

if ($samples.Count -gt 0) {
    $samples | Format-Table -AutoSize
    $wsAvg = ($samples.WorkingSetMB | Measure-Object -Average).Average
    $privAvg = ($samples.PrivateMB | Measure-Object -Average).Average
    "WorkingSet avg: {0:N1} MB" -f $wsAvg
    "Private    avg: {0:N1} MB" -f $privAvg
} else {
    Write-Warning "No memory samples collected"
}

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "ParentProcessId = $($proc.Id)" -ErrorAction SilentlyContinue |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Write-Host ""
Write-Host "Fly IDE processes stopped." -ForegroundColor Green
