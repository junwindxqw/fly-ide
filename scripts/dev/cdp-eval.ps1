#requires -Version 5.1
<#
.SYNOPSIS
    Evaluate a JS expression in the Fly IDE WebView2 page via Chrome DevTools Protocol.
.DESCRIPTION
    Requires Fly IDE started with:
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm dev
    NOTE: keep this file ASCII-only (Windows PowerShell 5.1 reads scripts as ANSI).
.PARAMETER Expression
    JavaScript expression, e.g. "document.body.innerText.slice(0,200)".
.PARAMETER Port
    Remote debugging port, default 9222.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev\cdp-eval.ps1 -Expression "window.__errors"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Expression,
    [int]$Port = 9222
)

$ErrorActionPreference = "Stop"

$targets = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/json/list" -TimeoutSec 5
$page = $targets | Where-Object { $_.type -eq "page" } | Select-Object -First 1
if (-not $page) {
    Write-Error "No page target found. Is Fly IDE running with --remote-debugging-port?"
    exit 1
}

$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ct = [System.Threading.CancellationToken]::None
$connect = $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, $ct)
if (-not $connect.Wait(5000)) { Write-Error "WebSocket connect timeout"; exit 1 }

$payload = @{
    id     = 1
    method = "Runtime.evaluate"
    params = @{ expression = $Expression; returnByValue = $true }
} | ConvertTo-Json -Depth 6 -Compress

$bytes = [Text.Encoding]::UTF8.GetBytes($payload)
$send = $ws.SendAsync((New-Object System.ArraySegment[byte] -ArgumentList $bytes, 0, $bytes.Length), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct)
$send.Wait(5000) | Out-Null

$buffer = New-Object byte[] (4 * 1024 * 1024)
$receive = $ws.ReceiveAsync((New-Object System.ArraySegment[byte] -ArgumentList $buffer, 0, $buffer.Length), $ct)
if (-not $receive.Wait(15000)) { Write-Error "Timeout waiting for CDP response"; exit 1 }

[Text.Encoding]::UTF8.GetString($buffer, 0, $receive.Result.Count)
$ws.Dispose()
