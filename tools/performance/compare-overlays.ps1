param(
    [ValidateRange(10, 3600)]
    [int]$DurationSeconds = 300,

    [ValidateRange(250, 5000)]
    [int]$SampleIntervalMs = 1000,

    [ValidateRange(0, 3600)]
    [int]$MemoryWarmupSeconds = 0,

    [string]$OutputDirectory = (Join-Path $PSScriptRoot "results")
)

$ErrorActionPreference = "Stop"
$logicalProcessors = [Environment]::ProcessorCount
$targets = @(
    [pscustomobject]@{ Name = "BlackRack Overlay"; ProcessNames = @("blackrack-overlay") },
    [pscustomobject]@{ Name = "TinyPedal"; ProcessNames = @("tinypedal") }
)

# WebView2 splits itself into a browser process, a GPU process, one renderer per
# window and several utilities. Adding them into a single number says how much
# the application costs but never which part holds it, so every process is also
# classified by its Chromium `--type=` switch. That switch never changes for a
# live process, so each command line is read once and cached.
$commandLineCache = @{}

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

function Update-CommandLineCache {
    param([int[]]$ProcessIds)

    $missing = @($ProcessIds | Where-Object { -not $commandLineCache.ContainsKey([int]$_) })
    if ($missing.Count -eq 0) { return }
    $filter = (($missing | ForEach-Object { "ProcessId=$_" }) -join " OR ")
    try {
        foreach ($row in @(Get-CimInstance Win32_Process -Filter $filter -ErrorAction Stop)) {
            $commandLineCache[[int]$row.ProcessId] = [string]$row.CommandLine
        }
    } catch {
    }
    # A process that ended between the tree walk and this query, or whose command
    # line is unreadable, is cached as empty so it is not queried every sample.
    foreach ($processId in $missing) {
        if (-not $commandLineCache.ContainsKey([int]$processId)) {
            $commandLineCache[[int]$processId] = ""
        }
    }
}

function Get-ProcessRole {
    param(
        [int]$ProcessId,
        [bool]$IsRoot
    )

    if ($IsRoot) { return "app" }
    $commandLine = [string]$commandLineCache[$ProcessId]
    if ($commandLine -match "--type=([\w-]+)") {
        switch ($Matches[1]) {
            "renderer" { return "renderer" }
            "gpu-process" { return "gpu" }
            "utility" { return "utility" }
            default { return "other" }
        }
    }
    # A WebView2 child without `--type=` is the browser process itself.
    if ($commandLine.Length -gt 0) { return "browser" }
    return "other"
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
$processesPath = Join-Path $OutputDirectory "overlay-performance-$stamp-processes.csv"
$previousCpu = @{}
$samples = [System.Collections.Generic.List[object]]::new()
$processStats = @{}
$stopwatch = [Diagnostics.Stopwatch]::StartNew()

Write-Host "Comparando BlackRack Overlay y TinyPedal durante $DurationSeconds s..."
Write-Host "Manten abiertos los mismos overlays/widgets y no cambies su configuracion durante la prueba."
if ($MemoryWarmupSeconds -gt 0) {
    Write-Host "Los primeros $MemoryWarmupSeconds s quedan fuera de las cifras de memoria; la CPU usa la captura entera."
}

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
        $rootIdSet = [System.Collections.Generic.HashSet[int]]::new()
        foreach ($rootId in $rootIds) { [void]$rootIdSet.Add($rootId) }
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

        $elapsedSeconds = $stopwatch.Elapsed.TotalSeconds
        $countsForMemory = $elapsedSeconds -ge $MemoryWarmupSeconds
        Update-CommandLineCache -ProcessIds @($running | ForEach-Object { [int]$_.Id })
        $roleMemory = @{ app = 0.0; browser = 0.0; gpu = 0.0; renderer = 0.0; utility = 0.0; other = 0.0 }
        $rendererMemory = @()
        foreach ($process in $running) {
            $processId = [int]$process.Id
            $privateMb = [double]$process.PrivateMemorySize64 / 1MB
            $role = Get-ProcessRole -ProcessId $processId -IsRoot $rootIdSet.Contains($processId)
            $roleMemory[$role] += $privateMb
            if ($role -eq "renderer") { $rendererMemory += $privateMb }

            $key = "$($target.Name)|$processId"
            $stat = $processStats[$key]
            if ($null -eq $stat) {
                $stat = [pscustomobject]@{
                    Application = $target.Name
                    ProcessId = $processId
                    Name = [string]$process.ProcessName
                    Role = $role
                    FirstSeenSeconds = [math]::Round($elapsedSeconds, 1)
                    LastSeenSeconds = [math]::Round($elapsedSeconds, 1)
                    Samples = 0
                    PrivateSum = 0.0
                    PrivateMax = 0.0
                    WorkingSetSum = 0.0
                }
                $processStats[$key] = $stat
            }
            $stat.LastSeenSeconds = [math]::Round($elapsedSeconds, 1)
            if ($countsForMemory) {
                $stat.Samples++
                $stat.PrivateSum += $privateMb
                $stat.WorkingSetSum += ([double]$process.WorkingSet64 / 1MB)
                if ($privateMb -gt $stat.PrivateMax) { $stat.PrivateMax = $privateMb }
            }
        }
        $rendererMax = if ($rendererMemory.Count) { ($rendererMemory | Measure-Object -Maximum).Maximum } else { 0.0 }

        $gpu = Get-GpuMetrics -ProcessIds $treeIds -EngineRows $gpuEngines -MemoryRows $gpuMemoryRows
        $samples.Add([pscustomobject]@{
            timestamp = (Get-Date).ToString("o")
            elapsed_seconds = [math]::Round($elapsedSeconds, 3)
            application = $target.Name
            running = $running.Count -gt 0
            root_pids = ($rootIds -join ";")
            process_count = $running.Count
            cpu_percent = if ($null -eq $cpuPercent) { $null } else { [math]::Round($cpuPercent, 3) }
            working_set_mb = [math]::Round([double](($running | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 2)
            private_memory_mb = [math]::Round([double](($running | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 2)
            private_app_mb = [math]::Round($roleMemory["app"], 2)
            private_browser_mb = [math]::Round($roleMemory["browser"], 2)
            private_gpu_process_mb = [math]::Round($roleMemory["gpu"], 2)
            private_renderer_mb = [math]::Round($roleMemory["renderer"], 2)
            private_utility_mb = [math]::Round($roleMemory["utility"], 2)
            private_other_mb = [math]::Round($roleMemory["other"], 2)
            renderer_count = $rendererMemory.Count
            renderer_max_mb = [math]::Round([double]$rendererMax, 2)
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
    # Memory ramps for about four minutes before it holds a plateau, so a reading
    # that includes the ramp reports a point on the warm-up curve instead of the
    # build. CPU needs no such window and keeps using every sample.
    $memoryRows = @($rows | Where-Object { [double]$_.elapsed_seconds -ge $MemoryWarmupSeconds })
    $cpu = @($rows | Where-Object { $null -ne $_.cpu_percent } | ForEach-Object { [double]$_.cpu_percent })
    $ram = @($memoryRows | ForEach-Object { [double]$_.private_memory_mb })
    $gpu = @($rows | ForEach-Object { [double]$_.gpu_max_engine_percent })
    [pscustomobject]@{
        application = $target.Name
        samples = $rows.Count
        cpu_average_percent = [math]::Round([double](($cpu | Measure-Object -Average).Average), 3)
        cpu_median_percent = [math]::Round((Get-Percentile -Values $cpu -Percentile 50), 3)
        cpu_p95_percent = [math]::Round((Get-Percentile -Values $cpu -Percentile 95), 3)
        cpu_max_percent = [math]::Round([double](($cpu | Measure-Object -Maximum).Maximum), 3)
        memory_warmup_seconds = $MemoryWarmupSeconds
        memory_samples = $memoryRows.Count
        private_memory_average_mb = [math]::Round([double](($ram | Measure-Object -Average).Average), 2)
        private_memory_max_mb = [math]::Round([double](($ram | Measure-Object -Maximum).Maximum), 2)
        private_app_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_app_mb -Average).Average), 2)
        private_browser_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_browser_mb -Average).Average), 2)
        private_gpu_process_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_gpu_process_mb -Average).Average), 2)
        private_renderer_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_renderer_mb -Average).Average), 2)
        private_utility_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_utility_mb -Average).Average), 2)
        private_other_average_mb = [math]::Round([double](($memoryRows | Measure-Object private_other_mb -Average).Average), 2)
        renderer_max_average_mb = [math]::Round([double](($memoryRows | Measure-Object renderer_max_mb -Average).Average), 2)
        renderer_count_max = [int](($memoryRows | Measure-Object renderer_count -Maximum).Maximum)
        gpu_average_percent = [math]::Round([double](($gpu | Measure-Object -Average).Average), 3)
        gpu_p95_percent = [math]::Round((Get-Percentile -Values $gpu -Percentile 95), 3)
        process_count_max = [int](($rows | Measure-Object process_count -Maximum).Maximum)
    }
}
$summary | Export-Csv -LiteralPath $summaryPath -NoTypeInformation -Encoding UTF8

$processRows = @($processStats.Values | Where-Object { $_.Samples -gt 0 } | ForEach-Object {
    [pscustomobject]@{
        application = $_.Application
        process_id = $_.ProcessId
        process_name = $_.Name
        role = $_.Role
        samples = $_.Samples
        first_seen_seconds = $_.FirstSeenSeconds
        last_seen_seconds = $_.LastSeenSeconds
        private_memory_average_mb = [math]::Round($_.PrivateSum / $_.Samples, 2)
        private_memory_max_mb = [math]::Round($_.PrivateMax, 2)
        working_set_average_mb = [math]::Round($_.WorkingSetSum / $_.Samples, 2)
    }
} | Sort-Object application, @{ Expression = "private_memory_average_mb"; Descending = $true })
$processRows | Export-Csv -LiteralPath $processesPath -NoTypeInformation -Encoding UTF8

Write-Host ""
$summary | Format-Table -AutoSize
Write-Host ""
$processRows | Format-Table -AutoSize application, process_id, role, private_memory_average_mb, private_memory_max_mb, working_set_average_mb
Write-Host "Muestras: $samplesPath"
Write-Host "Resumen:  $summaryPath"
Write-Host "Procesos: $processesPath"
