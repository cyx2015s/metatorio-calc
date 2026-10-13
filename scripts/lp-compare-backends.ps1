param(
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$Repeats = 2,
    [int]$TimeoutSeconds = 360
)
$ErrorActionPreference = 'Stop'
if ($Repeats -lt 1 -or $TimeoutSeconds -lt 1) { throw 'Invalid repeat count or timeout.' }
$root = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $root 'target/release/examples/lp_application_replay.exe'
if (!(Test-Path -LiteralPath $executable)) { throw 'Build lp_application_replay with lp-experiments first.' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a new evidence directory.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $OutputDirectory 'lp_application_replay.exe')
$executable = Join-Path $OutputDirectory 'lp_application_replay.exe'
$cases = @(Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json)
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $OutputDirectory 'manifest.json')
@{ git_head = (& git -C $root rev-parse HEAD); git_status = @(& git -C $root status --short);
    executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash;
    repeats = $Repeats; process_timeout_seconds = $TimeoutSeconds;
    note = 'Sequential production-adapter solves; setup excluded, captures included. Rejected solves are not timing wins.'
} | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8NoBOM (Join-Path $OutputDirectory 'provenance.json')
$results = [Collections.Generic.List[object]]::new()
for ($repeat = 0; $repeat -lt $Repeats; $repeat++) {
    foreach ($case in $cases) {
        # Reverse paired order on alternate repetitions.
        $backends = if ($repeat % 2 -eq 0) { @('highs', 'rust-certified') } else { @('rust-certified', 'highs') }
        $snapshot = [IO.Path]::GetFullPath($case.snapshot)
        $hash = (Get-FileHash -LiteralPath $snapshot -Algorithm SHA256).Hash
        foreach ($backend in $backends) {
            $folder = Join-Path $OutputDirectory "$($case.name)/repeat-$repeat/$backend"
            New-Item -ItemType Directory -Path $folder -Force | Out-Null
            $info = [Diagnostics.ProcessStartInfo]::new($executable)
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $info.WorkingDirectory = $root
            $info.ArgumentList.Add($snapshot)
            $info.ArgumentList.Add((Join-Path $folder 'result.json'))
            $info.Environment['METATORIO_LP_BACKEND'] = $backend
            $info.Environment['METATORIO_LP_CAPTURE_DIR'] = Join-Path $folder 'capture'
            $process = [Diagnostics.Process]::Start($info)
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            $clock = [Diagnostics.Stopwatch]::StartNew()
            $timedOut = $false
            try {
                while (!$process.WaitForExit(1000)) {
                    if ($clock.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                        $timedOut = $true
                        $process.Kill($true)
                        $process.WaitForExit()
                        break
                    }
                }
                $stdout.GetAwaiter().GetResult() | Set-Content -Encoding utf8NoBOM (Join-Path $folder 'stdout.log')
                $stderr.GetAwaiter().GetResult() | Set-Content -Encoding utf8NoBOM (Join-Path $folder 'stderr.log')
                $resultPath = Join-Path $folder 'result.json'
                $result = if (!$timedOut -and $process.ExitCode -eq 0 -and (Test-Path -LiteralPath $resultPath)) {
                    Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
                } else { [pscustomobject]@{ status = if ($timedOut) {'process_timeout'} else {'process_failed'}; elapsed_seconds = $null; cost = $null; report = $null } }
                $row = [pscustomobject]@{ name = $case.name; repeat = $repeat; backend = $backend;
                    snapshot_sha256 = $hash; status = $result.status; solve_seconds = $result.elapsed_seconds;
                    process_wall_seconds = $clock.Elapsed.TotalSeconds; cost = $result.cost;
                    report = $result.report; result_path = $resultPath }
                $results.Add($row)
                $results | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8NoBOM (Join-Path $OutputDirectory 'results.json')
                Write-Host "$($case.name) repeat=$repeat backend=$backend status=$($result.status) solve_seconds=$($result.elapsed_seconds)"
            } finally { $process.Dispose() }
        }
    }
}
