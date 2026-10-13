param(
    [string]$CorpusDirectory,
    [switch]$ReplayOnly,
    [switch]$PipelineOnly,
    [switch]$RequireSolved,
    [switch]$NativeFree,
    [switch]$UseDefaultBackend,
    [ValidateSet('highs', 'hybrid-exact', 'rust-exact', 'rust-verified', 'rust-certified')]
    [string]$Backend = 'highs',
    [ValidateSet('default', 'no-scaling', 'no-scaling-no-presolve', 'tight', 'tight-no-presolve', 'raw-rhs', 'ipx', 'ipx-no-presolve', 'support-refine')]
    [string]$Strategy = 'support-refine',
    [double]$TimeLimit = 20,
    [int]$RequestLimit = 24,
    [int]$RequestOffset = 0
)
$ErrorActionPreference = 'Stop'
if ($UseDefaultBackend -and (!$NativeFree -or !$RequireSolved -or $Backend -ne 'rust-certified')) {
    throw 'UseDefaultBackend requires NativeFree, RequireSolved and rust-certified expectations.'
}
if ($PipelineOnly -and $ReplayOnly) { throw 'PipelineOnly and ReplayOnly are mutually exclusive.' }
if ($RequestOffset -lt 0 -or $RequestLimit -lt 1) { throw 'Invalid request selection.' }
if ($NativeFree -and (!$PipelineOnly -or $ReplayOnly -or $Backend -notlike 'rust-*')) {
    throw 'NativeFree requires PipelineOnly and a Rust backend.'
}
$cargoFeatures = if ($UseDefaultBackend) { @() } elseif ($NativeFree) { @('--no-default-features', '--features', 'rust-lp') } else { @('--features', 'lp-experiments') }
$root = Split-Path -Parent $PSScriptRoot
if (!$CorpusDirectory) { $CorpusDirectory = Join-Path $root "local/lp-exp/run-$(Get-Date -Format yyyyMMdd-HHmmss)" }
$CorpusDirectory = [IO.Path]::GetFullPath($CorpusDirectory)
$names = @('CMAKE_GENERATOR', 'METATORIO_LP_CAPTURE_DIR', 'METATORIO_LP_BACKEND', 'LP_REQUIRE_SOLVED', 'LP_REQUEST_LIMIT', 'LP_REQUEST_OFFSET', 'LP_REQUESTS')
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $root
try {
    if (!$env:CMAKE_GENERATOR) { $env:CMAKE_GENERATOR = 'Ninja' }
    if (!$ReplayOnly) {
        if (Test-Path -LiteralPath $CorpusDirectory) { throw 'Capture requires a new directory, to preserve earlier evidence.' }
        New-Item -ItemType Directory -Path $CorpusDirectory | Out-Null
        $env:METATORIO_LP_CAPTURE_DIR = $CorpusDirectory
        $env:LP_REQUEST_LIMIT = "$RequestLimit"
        $env:LP_REQUEST_OFFSET = "$RequestOffset"
        if ($UseDefaultBackend) { Remove-Item Env:METATORIO_LP_BACKEND -ErrorAction SilentlyContinue } else { $env:METATORIO_LP_BACKEND = $Backend }
        $env:LP_REQUIRE_SOLVED = if ($RequireSolved) { '1' } else { '0' }
        $env:LP_REQUESTS = Join-Path $root 'crates/metatorio-shell/src/mcp/auto_plan_assets_tests/fixtures/lp-failure-requests.jsonl'
        @{
            captured_at = (Get-Date).ToUniversalTime().ToString('o')
            git_head = (& git rev-parse HEAD)
            git_status = @(& git status --short)
            rustc = (& rustc --version)
            assets_sha256 = (Get-FileHash -LiteralPath (Join-Path $root 'assets/data-raw-dump.json') -Algorithm SHA256).Hash
            requests_sha256 = (Get-FileHash -LiteralPath $env:LP_REQUESTS -Algorithm SHA256).Hash
            native_free_build = [bool]$NativeFree
            backend_environment_unset = [bool]$UseDefaultBackend
            default_features = [bool]$UseDefaultBackend
            backend = $Backend
            require_solved = [bool]$RequireSolved
            request_offset = $RequestOffset
            request_limit = $RequestLimit
        } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8NoBOM (Join-Path $CorpusDirectory 'provenance.json')
        if ($NativeFree) {
            $dependencyTree = @(& cargo tree -p metatorio-shell @cargoFeatures --edges normal,build --prefix none --format '{p}')
            if ($LASTEXITCODE -ne 0) { throw 'Dependency graph inspection failed.' }
            $dependencyTree | Set-Content -Encoding utf8NoBOM (Join-Path $CorpusDirectory 'dependency-tree.txt')
            if ($dependencyTree | Where-Object { $_ -match '^(highs(-sys)?|good_lp) v' }) {
                throw 'NativeFree dependency graph still contains a native LP backend.'
            }
        }
        & cargo test --release -p metatorio-shell @cargoFeatures --lib experiment_capture_failure_requests -- --ignored --nocapture *> (Join-Path $CorpusDirectory 'capture.log')
        if ($LASTEXITCODE -ne 0) { throw "Capture failed; see $CorpusDirectory/capture.log" }
    }
    if (!$ReplayOnly -and $RequireSolved -and $Backend -eq 'rust-certified') {
        & cargo build --release -p metatorio-solver @cargoFeatures --example lp_verify_witnesses
        if ($LASTEXITCODE -ne 0) { throw 'Exact witness verifier build failed.' }
        & (Join-Path $root 'target/release/examples/lp_verify_witnesses.exe') $CorpusDirectory | Set-Content -Encoding utf8NoBOM (Join-Path $CorpusDirectory 'witness-verification.json')
        if ($LASTEXITCODE -ne 0) { throw 'Independent exact witness verification failed.' }
    }
    if ($PipelineOnly) {
        Get-Content -LiteralPath (Join-Path $CorpusDirectory "request-outcomes-$RequestOffset.jsonl") | ForEach-Object {
            $case = $_ | ConvertFrom-Json
            [pscustomobject]@{ request_index = $case.request_index; status = $case.status; elapsed_ms = $case.elapsed_ms; error = $case.error_summary }
        } | Format-Table
        Write-Host "Evidence: $CorpusDirectory"
        return
    }
    & cargo build --release -p metatorio-solver --features lp-experiments --example lp_replay
    if ($LASTEXITCODE -ne 0) { throw 'Replay build failed.' }
    $outcomes = @(Get-Content -LiteralPath (Join-Path $CorpusDirectory "request-outcomes-$RequestOffset.jsonl") | ForEach-Object { $_ | ConvertFrom-Json })
    $replayDir = Join-Path $CorpusDirectory "replay-$Strategy-$(Get-Date -Format yyyyMMdd-HHmmss)"
    New-Item -ItemType Directory -Path $replayDir | Out-Null
    $summary = foreach ($case in $outcomes) {
        $caseDir = Join-Path $replayDir "request-$($case.request_index)"
        New-Item -ItemType Directory -Path $caseDir | Out-Null
        # Successful pipeline requests also capture small writeback subproblems.
        # Select the largest snapshot to replay the full candidate model.
        $snapshot = $case.snapshot_paths | Sort-Object { (Get-Item -LiteralPath $_).Length } -Descending | Select-Object -First 1
        & (Join-Path $root 'target/release/examples/lp_replay.exe') $snapshot $caseDir $Strategy $TimeLimit *> (Join-Path $caseDir 'replay.log')
        $resultPath = Join-Path $caseDir "$Strategy.json"
        if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $resultPath)) { throw "Replay failed; see $caseDir/replay.log" }
        $result = Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json
        if ($Strategy -eq 'support-refine') {
            $validation = $result.original_validation
            $status = $result.refined.status
            $columns = $result.selected_columns.Count
        } else {
            $validation = $result.validation
            $status = $result.status
            $columns = $result.values_bits.Count
        }
        [pscustomobject]@{
            request_index = $case.request_index; item = $case.request.item
            pipeline_status = $case.status; replay_status = $status; columns = $columns
            finite = $validation.finite; primal = $validation.maximum_relative_violation
            bounds = $validation.maximum_bound_violation; objective = $validation.objective
            # Explicit experimental bound budget; global optimality and writeback
            # are not established by this feasibility diagnostic.
            numerically_feasible = ($status -eq 'Optimal' -and $validation.finite -and $validation.maximum_relative_violation -le 1e-6 -and $validation.maximum_bound_violation -le 1e-8)
            snapshot_sha256 = (Get-FileHash -LiteralPath $snapshot -Algorithm SHA256).Hash
        }
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8NoBOM (Join-Path $replayDir 'summary.json')
    $summary | Format-Table request_index, item, pipeline_status, replay_status, columns, primal, bounds, numerically_feasible -AutoSize
    Write-Host "Evidence: $replayDir"
} finally {
    Pop-Location
    foreach ($name in $names) {
        if ($null -eq $saved[$name]) {
            Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
        } else {
            [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
        }
    }
}
