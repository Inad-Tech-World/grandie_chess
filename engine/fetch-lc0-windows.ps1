<#
.SYNOPSIS
Installs the official Windows build of Leela Chess Zero (lc0) as a Tauri sidecar.

.DESCRIPTION
Downloads lc0 v0.32.1 "onnx-dml" and the DirectML runtime it needs, checks
their SHA-256 hashes and installs:

  src-tauri/binaries/lc0-x86_64-pc-windows-msvc.exe   the sidecar
  src-tauri/binaries/windows/*.dll                    bundled next to lc0.exe
  src-tauri/binaries/windows/licenses/*               their licences

This lc0 build has two backends: onnx-dml (the GPU, through DirectML, on any
DirectX 12 card) and onnx-cpu (the processor, through ONNX Runtime). Its
source is the engine/lc0 submodule, which is pinned to the same tag.

The MSVC runtime DLLs that lc0 and ONNX Runtime need are copied from the
Visual Studio redistributable folder (or from System32 when Visual Studio is
not installed), so the app also runs on PCs without the VC++ redistributable.

.EXAMPLE
pwsh engine/fetch-lc0-windows.ps1

.EXAMPLE
powershell -ExecutionPolicy Bypass -File engine\fetch-lc0-windows.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest in Windows PowerShell 5.1 is very slow while it draws a progress bar.
$ProgressPreference = 'SilentlyContinue'

$Lc0Version = 'v0.32.1'
$Lc0Url = "https://github.com/LeelaChessZero/lc0/releases/download/$Lc0Version/lc0-$Lc0Version-windows-onnx-dml.zip"
$Lc0Sha256 = '86aac242b3a1c6986d02acc33b8cc64175b11a21b6941c62443c07fa18ac7539'
# The version the lc0 onnx-dml README asks for.
$DirectMLVersion = '1.15.4'
$DirectMLUrl = "https://globalcdn.nuget.org/packages/microsoft.ai.directml.$DirectMLVersion.nupkg"
$DirectMLSha256 = '4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9'
# MSVC runtime DLLs imported by lc0.exe and onnxruntime.dll.
$CrtDlls = @('msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')

$Root = Split-Path -Parent $PSScriptRoot
$BinDir = Join-Path $Root 'src-tauri\binaries'
$DllDir = Join-Path $BinDir 'windows'
$LicenseDir = Join-Path $DllDir 'licenses'
$Sidecar = Join-Path $BinDir 'lc0-x86_64-pc-windows-msvc.exe'

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem

function Get-Verified([string]$Url, [string]$Sha256, [string]$OutFile) {
  Write-Host "downloading $Url"
  Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $OutFile).Hash.ToLowerInvariant()
  if ($actual -ne $Sha256) {
    throw "checksum mismatch for $Url`n  expected $Sha256`n  got      $actual"
  }
}

# Extract one entry of a zip file (a .nupkg is a zip file too).
function Copy-FromZip([string]$Zip, [string]$Entry, [string]$Dest) {
  $archive = [System.IO.Compression.ZipFile]::OpenRead($Zip)
  try {
    $item = $archive.GetEntry($Entry)
    if (-not $item) { throw "$Entry is missing from $Zip" }
    [System.IO.Compression.ZipFileExtensions]::ExtractToFile($item, $Dest, $true)
  }
  finally {
    $archive.Dispose()
  }
}

# The newest Microsoft.VC14x.CRT folder of the Visual Studio C++ tools, if installed.
function Find-VcRedist {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
  $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if (-not $vs) { return $null }
  $redist = Join-Path $vs 'VC\Redist\MSVC'
  if (-not (Test-Path -LiteralPath $redist)) { return $null }
  $versions = Get-ChildItem -LiteralPath $redist -Directory |
    Sort-Object -Descending { $v = $null; if ([version]::TryParse($_.Name, [ref]$v)) { $v } else { [version]'0.0' } }
  foreach ($version in $versions) {
    $x64 = Join-Path $version.FullName 'x64'
    if (-not (Test-Path -LiteralPath $x64)) { continue }
    $crt = Get-ChildItem -LiteralPath $x64 -Directory -Filter 'Microsoft.VC14*.CRT' | Select-Object -First 1
    if ($crt) { return $crt.FullName }
  }
  return $null
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('grandie-lc0-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $lc0Zip = Join-Path $tmp 'lc0.zip'
  $dmlZip = Join-Path $tmp 'directml.zip'
  Get-Verified $Lc0Url $Lc0Sha256 $lc0Zip
  Get-Verified $DirectMLUrl $DirectMLSha256 $dmlZip

  # Stage lc0 with everything it loads, so it can be checked before installing.
  $stage = Join-Path $tmp 'stage'
  New-Item -ItemType Directory -Path $stage | Out-Null
  Copy-FromZip $lc0Zip 'lc0.exe' (Join-Path $stage 'lc0.exe')
  Copy-FromZip $lc0Zip 'onnxruntime.dll' (Join-Path $stage 'onnxruntime.dll')
  Copy-FromZip $dmlZip 'bin/x64-win/DirectML.dll' (Join-Path $stage 'DirectML.dll')

  $crtDir = Find-VcRedist
  if ($crtDir) {
    Write-Host "MSVC runtime from $crtDir"
  }
  else {
    $crtDir = Join-Path $env:SystemRoot 'System32'
    Write-Host "Visual Studio C++ tools not found; MSVC runtime from $crtDir"
  }
  foreach ($dll in $CrtDlls) {
    $src = Join-Path $crtDir $dll
    if (-not (Test-Path -LiteralPath $src)) {
      throw "$dll not found in $crtDir. Install the Visual Studio C++ build tools or the VC++ redistributable."
    }
    Copy-Item -LiteralPath $src -Destination $stage
  }

  # The backends this build offers are listed in its help ("VALUES: ...").
  # lc0 prints its banner on stderr, which Windows PowerShell 5.1 turns into
  # a terminating error under 'Stop', so relax that while it runs.
  $ErrorActionPreference = 'Continue'
  $help = & (Join-Path $stage 'lc0.exe') --help 2>&1 | Out-String
  $ErrorActionPreference = 'Stop'
  foreach ($backend in @('onnx-dml', 'onnx-cpu')) {
    if ($help -notmatch [regex]::Escape($backend)) {
      throw "lc0 does not offer the $backend backend. Its help output was:`n$help"
    }
  }
  Write-Host 'lc0 offers the onnx-dml and onnx-cpu backends'

  # Install.
  if (Test-Path -LiteralPath $DllDir) { Remove-Item -LiteralPath $DllDir -Recurse -Force }
  New-Item -ItemType Directory -Path $LicenseDir -Force | Out-Null
  Copy-Item -LiteralPath (Join-Path $stage 'lc0.exe') -Destination $Sidecar -Force
  Get-ChildItem -LiteralPath $stage -Filter '*.dll' | Copy-Item -Destination $DllDir

  Copy-FromZip $lc0Zip 'COPYING' (Join-Path $LicenseDir 'lc0-COPYING.txt')
  Copy-FromZip $lc0Zip 'ONNX-LICENSE' (Join-Path $LicenseDir 'onnxruntime-LICENSE.txt')
  Copy-FromZip $lc0Zip 'ONNX-ThirdPartyNotices.txt' (Join-Path $LicenseDir 'onnxruntime-ThirdPartyNotices.txt')
  Copy-FromZip $dmlZip 'LICENSE.txt' (Join-Path $LicenseDir 'DirectML-LICENSE.txt')
  Copy-FromZip $dmlZip 'ThirdPartyNotices.txt' (Join-Path $LicenseDir 'DirectML-ThirdPartyNotices.txt')

  Write-Host "installed $Sidecar"
  Get-ChildItem -LiteralPath $DllDir -Recurse -File | ForEach-Object {
    Write-Host ('  {0,-55} {1,12:N0} bytes' -f $_.FullName.Substring($BinDir.Length + 1), $_.Length)
  }
}
finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
