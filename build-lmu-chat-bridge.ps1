param(
    [string]$SdkPath = "E:\SteamLibrary\steamapps\common\Le Mans Ultimate\Support\SharedMemoryInterface",
    [string]$OutputDirectory = "$PSScriptRoot\dist"
)

$ErrorActionPreference = "Stop"

$header = Join-Path $SdkPath "InternalsPlugin.hpp"
if (-not (Test-Path -LiteralPath $header -PathType Leaf)) {
    throw "No se encontró InternalsPlugin.hpp en $SdkPath"
}

$vsDevCmd = Get-ChildItem `
    -Path "C:\Program Files (x86)\Microsoft Visual Studio\2022" `
    -Filter "VsDevCmd.bat" `
    -File `
    -Recurse `
    | Select-Object -First 1 -ExpandProperty FullName
if (-not $vsDevCmd) {
    throw "No se encontró Visual Studio Build Tools"
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$source = Join-Path $PSScriptRoot "src-tauri\src\telemetry\sim\lmu\chat_bridge_plugin.cpp"
$outputDll = Join-Path $OutputDirectory "LMU_BlackRackChatBridge.dll"
$outputLib = Join-Path $OutputDirectory "LMU_BlackRackChatBridge.lib"
$outputPdb = Join-Path $OutputDirectory "LMU_BlackRackChatBridge.pdb"
$outputObj = Join-Path $OutputDirectory "LMU_BlackRackChatBridge.obj"

$clCommand = @(
    "cl.exe /nologo /LD /std:c++17 /EHsc /W4 /WX /wd4100 /wd4201 /MT",
    " /I`"$SdkPath`" /I`"$(Split-Path $source)`" `"$source`"",
    " /Fo:`"$outputObj`" /Fe:`"$outputDll`" /link /NOLOGO /PDB:`"$outputPdb`" /IMPLIB:`"$outputLib`""
) -join ""
$command = "call `"$vsDevCmd`" -arch=x64 && $clCommand"

& cmd.exe /d /s /c $command
if ($LASTEXITCODE -ne 0) {
    throw "La compilación de LMU_BlackRackChatBridge.dll falló ($LASTEXITCODE)"
}

Write-Output $outputDll
