#requires -Version 5.1
<#
.SYNOPSIS
    Fly IDE Phase 0 benchmark: cold-start marks + process-tree memory.
.DESCRIPTION
    Launches Fly IDE, waits for the WebView2 child process, samples memory,
    prints startup marks from %LOCALAPPDATA%\FlyIDE\logs\startup.jsonl, then exits.
    Aborts if another fly-ide instance is running (two instances share the
    WebView2 user data dir and the second fails to create a webview).
    NOTE: keep this file ASCII-only (Windows PowerShell 5.1 reads scripts as ANSI).
.PARAMETER Exe
    Path to fly-ide.exe. Default: workspace release build (target\release\fly-ide.exe).
    Production-mode builds require: cargo build --release -p fly-ide --features tauri/custom-protocol
.PARAMETER Seconds
    Memory sampling duration in seconds. Default 10.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench\snapshot.ps1
#>
[CmdletBinding()]
param(
    [string]$Exe = "target\release\fly-ide.exe",
    [int]$Seconds = 10
)

$ErrorActionPreference = "Stop"

function Get-Median([double[]]$values) {
    $sorted = $values | Sort-Object
    $n = $sorted.Count
    if ($n -eq 0) { return 0.0 }
    if ($n % 2 -eq 1) { return $sorted[[int](($n - 1) / 2)] }
    return ($sorted[$n / 2 - 1] + $sorted[$n / 2]) / 2.0
}

function Stop-Tree([int]$ParentId) {
    $kids = Get-CimInstance Win32_Process -Filter "ParentProcessId = $ParentId" -ErrorAction SilentlyContinue
    foreach ($kid in $kids) {
        Stop-Tree -ParentId $kid.ProcessId
        Stop-Process -Id $kid.ProcessId -Force -ErrorAction SilentlyContinue
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$exePath = if ([System.IO.Path]::IsPathRooted($Exe)) { $Exe } else { Join-Path $repoRoot $Exe }
if (-not (Test-Path $exePath)) {
    Write-Error "Executable not found: $exePath`nBuild it first: pnpm bench:build (or cargo build --release -p fly-ide --features tauri/custom-protocol)"
    exit 1
}

$existing = Get-Process fly-ide -ErrorAction SilentlyContinue
if ($existing) {
    Write-Error ("A fly-ide instance is already running (PID {0}). Stop it first: two instances share the WebView2 user data dir and the second fails to create a webview." -f ($existing.Id -join ", "))
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

# WebView2 spawns helper processes (e.g. crashpad) before tauri setup completes;
# wait for the tauri.setup.end mark (max 5s) so the whole startup chain is visible.
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
Write-Host "-- Process tree memory (${Seconds}s, median of samples) --" -ForegroundColor Cyan
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
    $wsMedian = Get-Median ([double[]]$samples.WorkingSetMB)
    $privMedian = Get-Median ([double[]]$samples.PrivateMB)
    "WorkingSet median: {0:N1} MB" -f $wsMedian
    "Private    median: {0:N1} MB" -f $privMedian
} else {
    Write-Warning "No memory samples collected"
}

Stop-Tree -ParentId $proc.Id
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host ""
Write-Host "Fly IDE processes stopped." -ForegroundColor Green
