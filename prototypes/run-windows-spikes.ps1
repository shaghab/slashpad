<#
.SYNOPSIS
  Runs the SlashPad Milestone 0 Windows spikes and writes spike-results.txt.

.DESCRIPTION
  1. Records hardware/OS details.
  2. Renderer spike: startup and idle memory for empty / gdi / d2d / dwrite-gdi windows.
  3. Save spike: ReplaceFileW behaviour (TEMP, plus -SaveDir if given).
  4. Large-file spike: generates a -SizeGB file, then indexes it with the OS cache
     (buffered) and bypassing it (nobuf = cold read).
  5. IME spike (only with -Ime): opens a window for manual testing and appends its log.

  Needs the Rust toolchain (rustup + MSVC build tools). Run from any directory:
    powershell -ExecutionPolicy Bypass -File prototypes\run-windows-spikes.ps1

.EXAMPLE
  .\run-windows-spikes.ps1                       # everything except IME, 10 GB file
  .\run-windows-spikes.ps1 -Ime                  # also the interactive IME test
  .\run-windows-spikes.ps1 -SizeGB 1 -Runs 3     # quick run
  .\run-windows-spikes.ps1 -SaveDir \\server\share\tmp   # also test save on SMB
#>
[CmdletBinding()]
param(
    [int]$Runs = 10,
    [int]$SizeGB = 10,
    [string]$DataDir = (Join-Path $env:TEMP 'slashpad-testdata'),
    [string]$SaveDir = '',
    [switch]$SkipLarge,
    [switch]$KeepData,
    [switch]$Ime,
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$bin = Join-Path $root 'target\release'
$results = Join-Path $root 'spike-results.txt'
$work = Join-Path $env:TEMP 'slashpad-spikes'
New-Item -ItemType Directory -Force -Path $work | Out-Null
Set-Content -Path $results -Value "SlashPad M0 Windows spike results  $(Get-Date -Format s)" -Encoding UTF8

function Out-Result([string]$text) {
    Write-Host $text
    Add-Content -Path $results -Value $text -Encoding UTF8
}

function Get-Median([double[]]$values) {
    if ($values.Count -eq 0) { return [double]::NaN }
    $sorted = $values | Sort-Object
    $mid = [int][math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2) { return $sorted[$mid] }
    return ($sorted[$mid - 1] + $sorted[$mid]) / 2
}

function Invoke-WithTimeout([string]$exe, [string[]]$arguments) {
    # Windows PowerShell 5.1 does not quote -ArgumentList items: quote them so paths with spaces work.
    $quoted = ($arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $p = Start-Process -FilePath $exe -ArgumentList $quoted -PassThru -WindowStyle Normal
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        $p.Kill()
        throw "$exe timed out after $TimeoutSec s"
    }
    return $p.ExitCode
}

# --- 1. Environment ---------------------------------------------------------
Out-Result "`n## Environment"
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$cs = Get-CimInstance Win32_ComputerSystem
Out-Result ("OS:      {0} build {1} ({2})" -f $os.Caption, $os.BuildNumber, $os.OSArchitecture)
Out-Result ("CPU:     {0}, {1} cores / {2} threads" -f $cpu.Name.Trim(), $cpu.NumberOfCores, $cpu.NumberOfLogicalProcessors)
Out-Result ("RAM:     {0:N1} GB" -f ($cs.TotalPhysicalMemory / 1GB))
try {
    $drive = (Split-Path -Qualifier $DataDir).TrimEnd(':')
    $disk = Get-Partition -DriveLetter $drive | Get-Disk
    $phys = Get-PhysicalDisk | Where-Object DeviceId -eq $disk.Number
    Out-Result ("Disk:    {0} ({1}, {2}) for {3}:" -f $disk.FriendlyName, $phys.MediaType, $phys.BusType, $drive)
} catch { Out-Result "Disk:    (could not determine: $($_.Exception.Message))" }
try {
    Add-Type -AssemblyName System.Windows.Forms
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen
    Out-Result ("Display: primary {0}x{1}" -f $screen.Bounds.Width, $screen.Bounds.Height)
} catch { }
Out-Result ("Rust:    {0}" -f (& rustc --version))

# --- Build -------------------------------------------------------------------
Out-Result "`n## Build"
Push-Location $root
try {
    & cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }
foreach ($exe in 'm0-win-render', 'm0-win-save', 'm0-win-ime', 'm0-scan') {
    $f = Get-Item (Join-Path $bin "$exe.exe")
    Out-Result ("{0,-14} {1,8:N0} KB" -f $exe, ($f.Length / 1KB))
}

# --- 2. Renderer spike ---------------------------------------------------------
Out-Result "`n## Renderer spike (risk R-01): median of $Runs runs after 1 warm-up, idle 1.5 s"
$modes = 'empty', 'gdi', 'd2d', 'dwrite-gdi'
$jsonl = Join-Path $work 'render.jsonl'
Remove-Item $jsonl -ErrorAction SilentlyContinue
$renderExe = Join-Path $bin 'm0-win-render.exe'
foreach ($m in $modes) { Invoke-WithTimeout $renderExe @($m, "$work\warmup.jsonl", '500') | Out-Null }
for ($i = 1; $i -le $Runs; $i++) {
    foreach ($m in $modes) {   # interleaved so drift affects all modes equally
        Invoke-WithTimeout $renderExe @($m, $jsonl, '1500') | Out-Null
    }
}
$rows = Get-Content $jsonl | ForEach-Object { $_ | ConvertFrom-Json }
Out-Result ("{0,-11} {1,15} {2,16} {3,18} {4,14} {5,14}" -f 'mode', 'first_paint_ms', 'private_ws_MB', 'private_commit_MB', 'total_ws_MB', 'runs')
foreach ($m in $modes) {
    $r = @($rows | Where-Object { $_.mode -eq $m -and -not $_.error })
    Out-Result ("{0,-11} {1,15:N1} {2,16:N2} {3,18:N2} {4,14:N2} {5,14}" -f $m,
        (Get-Median ($r | ForEach-Object { [double]$_.first_paint_ms })),
        ((Get-Median ($r | ForEach-Object { [double]$_.private_ws_kb })) / 1024),
        ((Get-Median ($r | ForEach-Object { [double]$_.private_commit_kb })) / 1024),
        ((Get-Median ($r | ForEach-Object { [double]$_.ws_kb })) / 1024),
        $r.Count)
}
Out-Result "Raw rows: $jsonl"

# --- 3. Save spike -------------------------------------------------------------
Out-Result "`n## Save spike (risk R-05)"
$saveExe = Join-Path $bin 'm0-win-save.exe'
$saveDirs = @(Join-Path $work 'save')
if ($SaveDir) { $saveDirs += $SaveDir }
foreach ($d in $saveDirs) {
    $out = & $saveExe $d | Out-String
    Out-Result $out.TrimEnd()
    Out-Result "exit code: $LASTEXITCODE"
}

# --- 4. Large-file spike -------------------------------------------------------
if (-not $SkipLarge) {
    Out-Result "`n## Large-file spike: $SizeGB GB, generated log text"
    New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
    $free = (Get-PSDrive -Name (Split-Path -Qualifier $DataDir).TrimEnd(':')).Free
    if ($free -lt ($SizeGB + 2) * 1GB) {
        Out-Result ("SKIPPED: only {0:N1} GB free on {1}" -f ($free / 1GB), $DataDir)
    } else {
        $scan = Join-Path $bin 'm0-scan.exe'
        $file = Join-Path $DataDir "spike-${SizeGB}g.log"
        Out-Result (& $scan gen $file ($SizeGB * 1024) | Out-String).TrimEnd()
        Out-Result "-- buffered read (likely partly cached right after generation):"
        Out-Result (& $scan index $file | Out-String).TrimEnd()
        Out-Result "-- unbuffered read (bypasses the OS cache = cold read):"
        Out-Result (& $scan nobuf $file | Out-String).TrimEnd()
        Out-Result (& $scan nobuf $file | Out-String).TrimEnd()
        $long = Join-Path $DataDir 'spike-1g-single-line.txt'
        Out-Result "-- 1 GB file with a single line, unbuffered:"
        & $scan gen $long 1024 long | Out-Null
        Out-Result (& $scan nobuf $long | Out-String).TrimEnd()
        if (-not $KeepData) { Remove-Item $file, $long -ErrorAction SilentlyContinue }
    }
}

# --- 5. IME spike --------------------------------------------------------------
if ($Ime) {
    Out-Result "`n## IME spike (risk R-02): manual"
    Write-Host "`nA window will open. Follow the checklist in prototypes\README.md, then close the window." -ForegroundColor Yellow
    $log = Join-Path $work 'ime.log'
    $p = Start-Process -FilePath (Join-Path $bin 'm0-win-ime.exe') -ArgumentList ('"' + $log + '"') -PassThru
    $p.WaitForExit()
    Out-Result "IME log (first 400 lines):"
    Get-Content $log -TotalCount 400 | ForEach-Object { Add-Content -Path $results -Value $_ -Encoding UTF8 }
    Write-Host "Now fill in the checklist answers at the end of $results." -ForegroundColor Yellow
    Add-Content -Path $results -Value @"

IME checklist (replace ? with yes/no and add notes):
[?] Japanese: composition text appears inline at the caret, underlined
[?] Japanese: candidate window appears just below the caret
[?] Japanese: Enter commits; committed text appears exactly once
[?] Japanese: Backspace while composing edits the composition, not the text
[?] Chinese (Pinyin): candidates and commit work as above
[?] Korean (if available): Hangul composition works as above
[?] Esc cancels composition without inserting anything
[?] Moving the window during composition keeps the candidate window attached
Notes:
"@ -Encoding UTF8
}

Out-Result "`nDone. Please paste the contents of: $results"
