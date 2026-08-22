param(
    [Parameter(Mandatory = $true)]
    [string] $ElsaRepo,

    [string] $CucExe = (Join-Path $PSScriptRoot "..\target\release\cuc.exe"),

    [string[]] $Commit = @(
        "707462a1", "0c89eafe", "4885d63f", "77333053", "feb665cf", "5ab9d74d",
        "ccb349cd", "5943da6f", "fb35adf8", "e49644da", "77f08898", "11c8b506",
        "99f358a9", "d41d2cbf", "7ca522416", "3c9b17764", "24cd81bbd", "30f92748c"
    ),

    [string[]] $BaseRevision = @("HEAD"),

    [string] $ReportDirectory,

    [switch] $CurrentOnly,

    [switch] $KeepGoing
)

$ErrorActionPreference = "Stop"

$elsa = (Resolve-Path $ElsaRepo).Path
$cuc = (Resolve-Path $CucExe).Path
$jb = (Get-Command jb -ErrorAction Stop).Source
$msbuild = (Get-Command MSBuild -ErrorAction Stop).Source
$hadMismatch = $false
$targets = @(
    if (-not $CurrentOnly) {
        foreach ($revision in $Commit) {
            [pscustomobject]@{
                Label = "before-$revision"
                Revision = "$revision^"
            }
        }
    }
    foreach ($revision in $BaseRevision) {
        [pscustomobject]@{
            Label = "at-$revision"
            Revision = $revision
        }
    }
)

if ($ReportDirectory) {
    New-Item -ItemType Directory -Force -Path $ReportDirectory | Out-Null
    $ReportDirectory = (Resolve-Path $ReportDirectory).Path
}

foreach ($target in $targets) {
    $label = $target.Label
    $revision = $target.Revision
    $safeLabel = $label -replace '[^A-Za-z0-9_.-]', '_'
    $runRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("cuc-parity-" + [Guid]::NewGuid())
    $candidate = Join-Path $runRoot "candidate"
    $resharper = Join-Path $runRoot "resharper"
    $keepReport = $false
    New-Item -ItemType Directory -Path $runRoot | Out-Null

    try {
        & git -C $elsa worktree add --detach $candidate $revision
        if ($LASTEXITCODE -ne 0) { throw "failed to create candidate worktree" }
        & git -C $elsa worktree add --detach $resharper $revision
        if ($LASTEXITCODE -ne 0) { throw "failed to create ReSharper worktree" }

        & $cuc --config (Join-Path $candidate ".editorconfig") --text --csharp (Join-Path $candidate "Elsa.sln")
        if ($LASTEXITCODE -ne 0) { throw "cuc failed for $label" }
        $firstCucDiff = (& git -C $candidate diff --no-ext-diff --binary -- .) -join "`n"
        & $cuc --config (Join-Path $candidate ".editorconfig") --text --csharp (Join-Path $candidate "Elsa.sln")
        if ($LASTEXITCODE -ne 0) { throw "second cuc run failed for $label" }
        $secondCucDiff = (& git -C $candidate diff --no-ext-diff --binary -- .) -join "`n"
        if ($firstCucDiff -ne $secondCucDiff) {
            $hadMismatch = $true
            $reportRoot = if ($ReportDirectory) { $ReportDirectory } else { $runRoot }
            $report = Join-Path $reportRoot "$safeLabel.cuc-idempotency.diff"
            @(
                "=== cuc first run ===",
                $firstCucDiff,
                "",
                "=== cuc second run ===",
                $secondCucDiff
            ) | Out-File -Encoding utf8 $report
            $keepReport = -not $ReportDirectory
            Write-Error "cuc is not idempotent for $label. Report: $report" -ErrorAction Continue
            if (-not $KeepGoing) {
                throw "cuc is not idempotent for $label"
            }
        }

        Push-Location $resharper
        try {
            & $msbuild ./Elsa.sln /t:Restore /p:RestoreLockedMode=true
            if ($LASTEXITCODE -ne 0) { throw "locked-mode restore failed for $label" }

            & $jb cleanupcode ./Elsa.sln --config-file=.editorconfig --settings=Elsa.sln.DotSettings --no-build --severity=WARNING '--exclude=**/*.html'
            if ($LASTEXITCODE -ne 0) { throw "cleanupcode failed for $label" }
            $firstReSharperDiff = (& git diff --no-ext-diff --binary -- .) -join "`n"
            & $jb cleanupcode ./Elsa.sln --config-file=.editorconfig --settings=Elsa.sln.DotSettings --no-build --severity=WARNING '--exclude=**/*.html'
            if ($LASTEXITCODE -ne 0) { throw "second cleanupcode run failed for $label" }
            $secondReSharperDiff = (& git diff --no-ext-diff --binary -- .) -join "`n"
            if ($firstReSharperDiff -ne $secondReSharperDiff) {
                $hadMismatch = $true
                $reportRoot = if ($ReportDirectory) { $ReportDirectory } else { $runRoot }
                $report = Join-Path $reportRoot "$safeLabel.resharper-idempotency.diff"
                @(
                    "=== ReSharper first run ===",
                    $firstReSharperDiff,
                    "",
                    "=== ReSharper second run ===",
                    $secondReSharperDiff
                ) | Out-File -Encoding utf8 $report
                $keepReport = -not $ReportDirectory
                Write-Error "cleanupcode is not idempotent for $label. Report: $report" -ErrorAction Continue
                if (-not $KeepGoing) {
                    throw "cleanupcode is not idempotent for $label"
                }
            }
        }
        finally {
            Pop-Location
        }

        $cucDiff = $secondCucDiff
        $resharperDiff = $secondReSharperDiff
        if ($cucDiff -ne $resharperDiff) {
            $hadMismatch = $true
            $reportRoot = if ($ReportDirectory) { $ReportDirectory } else { $runRoot }
            $report = Join-Path $reportRoot "$safeLabel.parity.diff"
            @(
                "=== cuc ===",
                $cucDiff,
                "",
                "=== ReSharper cleanupcode ===",
                $resharperDiff
            ) | Out-File -Encoding utf8 $report
            $keepReport = -not $ReportDirectory
            Write-Error "Parity mismatch for $label. Report: $report" -ErrorAction Continue
            if (-not $KeepGoing) {
                throw "parity mismatch for $label"
            }
        }
        else {
            Write-Host "Parity matched: $label ($revision)"
        }
    }
    finally {
        & git -C $elsa worktree remove $candidate --force 2>$null
        & git -C $elsa worktree remove $resharper --force 2>$null
        if ((Test-Path $runRoot) -and -not $keepReport) {
            Remove-Item -Recurse -Force $runRoot
        }
    }
}

if ($hadMismatch) {
    throw "one or more parity comparisons failed"
}
