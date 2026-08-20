param(
    [ValidateRange(10, 3600)]
    [int]$DurationSeconds = 300,

    [ValidateRange(250, 5000)]
    [int]$SampleIntervalMs = 1000,

    [string]$OutputDirectory = (Join-Path $PSScriptRoot "results")
)

$ErrorActionPreference = "Stop"
$logicalProcessors = [Environment]::ProcessorCount
$targets = @(
    [pscustomobject]@{ Name = "BlackRack Overlay"; ProcessNames = @("blackrack-overlay") },
    [pscustomobject]@{ Name = "TinyPedal"; ProcessNames = @("tinypedal") }
)

function Get-ProcessTreeIds {
    param(
        [object[]]$ProcessTable,
        [int[]]$RootIds
    )

    $selected = [System.Collections.Generic.HashSet[int]]::new()
    foreach ($rootId in $RootIds) { [void]$selected.Add($rootId) }
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($process in $ProcessTable) {
            if ($selected.Contains([int]$process.ParentProcessId) -and $selected.Add([int]$process.ProcessId)) {
                $changed = $true
            }
        }
    }
    return @($selected)
}

function Get-GpuMetrics {
    param(
        [int[]]$ProcessIds,
        [object[]]$EngineRows,
        [object[]]$MemoryRows
    )

    if ($ProcessIds.Count -eq 0) {
        return [pscustomobject]@{ SumPercent = 0.0; MaxEnginePercent = 0.0; MemoryMb = 0.0 }
    }

    $idSet = [System.Collections.Generic.HashSet[int]]::new()
    foreach ($processId in $ProcessIds) { [void]$idSet.Add($processId) }
    $engineValues = @()
    foreach ($engine in $EngineRows) {
        if ($engine.Name -match "pid_(\d+)_" -and $idSet.Contains([int]$Matches[1])) {
            $engineValues += [double]$engine.UtilizationPercentage
        }
    }

    $gpuMemory = 0.0
    foreach ($memory in $MemoryRows) {
        if ($memory.Name -match "pid_(\d+)_" -and $idSet.Contains([int]$Matches[1])) {
            $gpuMemory += ([double]$memory.DedicatedUsage + [double]$memory.SharedUsage)
        }
    }

    $sum = if ($engineValues.Count) { ($engineValues | Measure-Object -Sum).Sum } else { 0.0 }
    $maximum = if ($engineValues.Count) { ($engineValues | Measure-Object -Maximum).Maximum } else { 0.0 }
    return [pscustomobject]@{
        SumPercent = [math]::Round([double]$sum, 3)
        MaxEnginePercent = [math]::Round([double]$maximum, 3)
        MemoryMb = [math]::Round($gpuMemory / 1MB, 2)
    }
}

function Get-Percentile {
    param([double[]]$Values, [double]$Percentile)
    if ($Values.Count -eq 0) { return 0.0 }
    $ordered = @($Values | Sort-Object)
    $index = [math]::Ceiling(($Percentile / 100.0) * $ordered.Count) - 1
    return [double]$ordered[[math]::Max(0, [math]::Min($index, $ordered.Count - 1))]
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$samplesPath = Join-Path $OutputDirectory "overlay-performance-$stamp.csv"
$summaryPath = Join-Path $OutputDirectory "overlay-performance-$stamp-summary.csv"
$previousCpu = @{}
$samples = [System.Collections.Generic.List[object]]::new()
$stopwatch = [Diagnostics.Stopwatch]::StartNew()

Write-Host "Comparando BlackRack Overlay y TinyPedal durante $DurationSeconds s..."
Write-Host "Mantén abiertos los mismos overlays/widgets y no cambies su configuración durante la prueba."

while ($stopwatch.Elapsed.TotalSeconds -lt $DurationSeconds) {
    $sampleStarted = [Diagnostics.Stopwatch]::StartNew()
    $processTable = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name)
    try {
        $gpuEngines = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop)
        $gpuMemoryRows = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction Stop)
    } catch {
        $gpuEngines = @()
        $gpuMemoryRows = @()
    }

    foreach ($target in $targets) {
        $rootIds = @($processTable | Where-Object {
            $name = [IO.Path]::GetFileNameWithoutExtension([string]$_.Name)
            $target.ProcessNames -contains $name
        } | ForEach-Object { [int]$_.ProcessId })
        $treeIds = @(Get-ProcessTreeIds -ProcessTable $processTable -RootIds $rootIds)
        $running = @($treeIds | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
        $cpuTotal = [double](($running | Measure-Object -Property CPU -Sum).Sum)
        $previous = $previousCpu[$target.Name]
        $cpuPercent = $null
        if ($null -ne $previous -and $cpuTotal -ge [double]$previous.CpuTotal) {
            $elapsed = [math]::Max($stopwatch.Elapsed.TotalSeconds - [double]$previous.Elapsed, 0.001)
            $cpuPercent = (($cpuTotal - [double]$previous.CpuTotal) / $elapsed / $logicalProcessors) * 100.0
        }
        $previousCpu[$target.Name] = [pscustomobject]@{
            CpuTotal = $cpuTotal
            Elapsed = $stopwatch.Elapsed.TotalSeconds
        }

        $gpu = Get-GpuMetrics -ProcessIds $treeIds -EngineRows $gpuEngines -MemoryRows $gpuMemoryRows
        $samples.Add([pscustomobject]@{
            timestamp = (Get-Date).ToString("o")
            elapsed_seconds = [math]::Round($stopwatch.Elapsed.TotalSeconds, 3)
            application = $target.Name
            running = $running.Count -gt 0
            root_pids = ($rootIds -join ";")
            process_count = $running.Count
            cpu_percent = if ($null -eq $cpuPercent) { $null } else { [math]::Round($cpuPercent, 3) }
            working_set_mb = [math]::Round([double](($running | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 2)
            private_memory_mb = [math]::Round([double](($running | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 2)
            thread_count = [int](($running | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum)
            handle_count = [int](($running | Measure-Object HandleCount -Sum).Sum)
            gpu_sum_percent = $gpu.SumPercent
            gpu_max_engine_percent = $gpu.MaxEnginePercent
            gpu_memory_mb = $gpu.MemoryMb
        })
    }

    $remainingDelay = $SampleIntervalMs - $sampleStarted.ElapsedMilliseconds
    if ($remainingDelay -gt 0) { Start-Sleep -Milliseconds $remainingDelay }
}

$samples | Export-Csv -LiteralPath $samplesPath -NoTypeInformation -Encoding UTF8
$summary = foreach ($target in $targets) {
    $rows = @($samples | Where-Object { $_.application -eq $target.Name -and $_.running })
    $cpu = @($rows | Where-Object { $null -ne $_.cpu_percent } | ForEach-Object { [double]$_.cpu_percent })
    $ram = @($rows | ForEach-Object { [double]$_.private_memory_mb })
    $gpu = @($rows | ForEach-Object { [double]$_.gpu_max_engine_percent })
    [pscustomobject]@{
        application = $target.Name
        samples = $rows.Count
        cpu_average_percent = [math]::Round([double](($cpu | Measure-Object -Average).Average), 3)
        cpu_median_percent = [math]::Round((Get-Percentile -Values $cpu -Percentile 50), 3)
        cpu_p95_percent = [math]::Round((Get-Percentile -Values $cpu -Percentile 95), 3)
        cpu_max_percent = [math]::Round([double](($cpu | Measure-Object -Maximum).Maximum), 3)
        private_memory_average_mb = [math]::Round([double](($ram | Measure-Object -Average).Average), 2)
        private_memory_max_mb = [math]::Round([double](($ram | Measure-Object -Maximum).Maximum), 2)
        gpu_average_percent = [math]::Round([double](($gpu | Measure-Object -Average).Average), 3)
        gpu_p95_percent = [math]::Round((Get-Percentile -Values $gpu -Percentile 95), 3)
        process_count_max = [int](($rows | Measure-Object process_count -Maximum).Maximum)
    }
}
$summary | Export-Csv -LiteralPath $summaryPath -NoTypeInformation -Encoding UTF8

Write-Host ""
$summary | Format-Table -AutoSize
Write-Host "Muestras: $samplesPath"
Write-Host "Resumen:  $summaryPath"
