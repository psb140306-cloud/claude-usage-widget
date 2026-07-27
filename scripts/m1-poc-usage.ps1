<#
.SYNOPSIS
  M1 PoC — Claude Code OAuth 자격증명을 재사용해 비공식 usage 엔드포인트를 호출한다.

.DESCRIPTION
  %USERPROFILE%\.claude\.credentials.json 에서 accessToken 을 읽기 전용으로 로드한 뒤
  GET https://api.anthropic.com/api/oauth/usage 를 호출하고 응답을 요약/원문 출력한다.

  보안: accessToken 은 어떤 경로로도 출력·저장하지 않는다.
        (-Raw 로 저장하는 응답 원문에는 토큰이 포함되지 않는다.)

.PARAMETER Raw
  응답 JSON 원문을 그대로 출력한다.

.PARAMETER OutFile
  응답 JSON 원문을 지정 파일에 저장한다.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\m1-poc-usage.ps1
  powershell -ExecutionPolicy Bypass -File scripts\m1-poc-usage.ps1 -Raw
#>
[CmdletBinding()]
param(
    [switch] $Raw,
    [string] $OutFile
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$UsageUrl   = 'https://api.anthropic.com/api/oauth/usage'
$OAuthBeta  = 'oauth-2025-04-20'
$CredPath   = Join-Path $env:USERPROFILE '.claude\.credentials.json'

# ---------------------------------------------------------------- credentials
# 모듈 경계: credentials.load() -> Token | NotFound | Expired | ParseError (PRD 5.3)
function Get-ClaudeCredentials {
    if (-not (Test-Path $CredPath)) {
        return @{ Status = 'NotFound'; Detail = $CredPath }
    }
    try {
        $json = Get-Content $CredPath -Raw | ConvertFrom-Json
    } catch {
        return @{ Status = 'ParseError'; Detail = $_.Exception.Message }
    }

    $oauth = $json.claudeAiOauth
    if (-not $oauth -or [string]::IsNullOrEmpty($oauth.accessToken)) {
        return @{ Status = 'ParseError'; Detail = 'claudeAiOauth.accessToken 없음' }
    }

    # expiresAt 은 epoch milliseconds
    $expiresAt = [DateTimeOffset]::FromUnixTimeMilliseconds([int64]$oauth.expiresAt)
    $status = if ($expiresAt -le [DateTimeOffset]::UtcNow) { 'Expired' } else { 'Ok' }

    return @{
        Status           = $status
        Token            = $oauth.accessToken   # 절대 출력하지 않는다
        ExpiresAt        = $expiresAt
        SubscriptionType = $oauth.subscriptionType
        RateLimitTier    = $oauth.rateLimitTier
        Scopes           = $oauth.scopes
    }
}

# ---------------------------------------------------------------- usage client
# 모듈 경계: usage-client.fetchUsage(token) -> Snapshot | AuthError | NetworkError (PRD 5.3)
function Invoke-UsageFetch {
    param([Parameter(Mandatory)][string] $Token)

    $headers = @{
        'Authorization'  = "Bearer $Token"
        'anthropic-beta' = $OAuthBeta
        'Content-Type'   = 'application/json'
        'Accept'         = 'application/json'
    }
    # -UseBasicParsing: PS 5.1 이 IE 엔진을 쓰려다 NonInteractive 에서 실패하는 것 방지
    $resp = Invoke-WebRequest -Uri $UsageUrl -Method GET -Headers $headers `
                              -TimeoutSec 20 -UseBasicParsing
    return $resp.Content
}

function Format-Limit {
    param($Label, $Node)
    if ($null -eq $Node -or $null -eq $Node.utilization) { return $null }
    $reset = if ($Node.resets_at) {
        ([DateTimeOffset]::Parse($Node.resets_at)).ToLocalTime().ToString('yyyy-MM-dd HH:mm')
    } else { 'n/a' }
    return ('{0,-24} {1,5:N1}%   resets {2}' -f $Label, $Node.utilization, $reset)
}

# ---------------------------------------------------------------------- main
$cred = Get-ClaudeCredentials

switch ($cred.Status) {
    'NotFound'   { Write-Host "[FAIL] 자격증명 파일 없음: $($cred.Detail)" -ForegroundColor Red; exit 2 }
    'ParseError' { Write-Host "[FAIL] 자격증명 파싱 실패: $($cred.Detail)" -ForegroundColor Red; exit 2 }
    'Expired'    { Write-Host "[WARN] 토큰 만료됨 ($($cred.ExpiresAt.ToLocalTime())). Claude Code 를 한 번 실행해 갱신하세요." -ForegroundColor Yellow }
}

Write-Host "credentials : OK  (plan=$($cred.SubscriptionType), tier=$($cred.RateLimitTier))"
Write-Host "token expires: $($cred.ExpiresAt.ToLocalTime().ToString('yyyy-MM-dd HH:mm'))"
Write-Host ''

try {
    $body = Invoke-UsageFetch -Token $cred.Token
} catch {
    $resp = $_.Exception.Response
    if ($resp) {
        $code = [int] $resp.StatusCode
        $sr   = New-Object System.IO.StreamReader($resp.GetResponseStream())
        $text = $sr.ReadToEnd()
        if ($code -eq 401) {
            Write-Host "[FAIL] 401 재인증 필요 — Claude Code 를 실행해 토큰을 갱신하세요." -ForegroundColor Red
        } else {
            Write-Host "[FAIL] HTTP $code`n$text" -ForegroundColor Red
        }
        exit 3
    }
    Write-Host "[FAIL] 네트워크 오류: $($_.Exception.Message)" -ForegroundColor Red
    exit 3
}

if ($OutFile) {
    $body | Out-File -FilePath $OutFile -Encoding utf8
    Write-Host "응답 원문 저장: $OutFile"
}

$u = $body | ConvertFrom-Json

Write-Host 'usage (' -NoNewline; Write-Host $UsageUrl -NoNewline; Write-Host ') → HTTP 200'
Write-Host ('-' * 60)

# 1) 명시적 버킷 (five_hour / seven_day / seven_day_opus …)
$named = [ordered]@{
    'session (5h)'        = $u.five_hour
    'weekly (all models)' = $u.seven_day
    'weekly (Opus)'       = $u.seven_day_opus
    'weekly (Sonnet)'     = $u.seven_day_sonnet
    'weekly (OAuth apps)' = $u.seven_day_oauth_apps
}
foreach ($k in $named.Keys) {
    $line = Format-Limit $k $named[$k]
    if ($line) { Write-Host $line }
}

# 2) limits[] 배열 (모델 스코프 버킷 포함) — 서버가 추가 버킷을 늘려도 여기에 나타난다
if ($u.limits) {
    Write-Host ''
    Write-Host 'limits[]:'
    foreach ($l in $u.limits) {
        $model  = if ($l.scope -and $l.scope.model) { " [$($l.scope.model.display_name)]" } else { '' }
        $active = if ($l.is_active) { ' *active' } else { '' }
        $reset  = if ($l.resets_at) {
            ([DateTimeOffset]::Parse($l.resets_at)).ToLocalTime().ToString('yyyy-MM-dd HH:mm')
        } else { 'n/a' }
        Write-Host ('  {0,-16}{1,-10} {2,4}%  severity={3,-8} resets {4}{5}' -f `
                    $l.kind, $model, $l.percent, $l.severity, $reset, $active)
    }
}

if ($Raw) {
    Write-Host ''
    Write-Host '--- RAW ---'
    Write-Host $body
}
