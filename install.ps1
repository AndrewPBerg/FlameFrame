param(
    [string]$Version = "latest",
    [string]$Repo = "AndrewPBerg/FlameFrame",
    [string]$InstallDir = "",
    [switch]$NoPathUpdate
)

$ErrorActionPreference = "Stop"

if (-not $InstallDir) {
    if ($env:LOCALAPPDATA) {
        $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\FlameFrame"
    } else {
        $InstallDir = Join-Path $HOME ".flameframe\bin"
    }
}

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "Unsupported Windows architecture: $arch. Release binaries currently ship for x64 Windows."
}

$target = "x86_64-pc-windows-msvc"
$asset = "flameframe-$target.zip"
if ($Version -eq "latest") {
    $url = "https://github.com/$Repo/releases/latest/download/$asset"
} else {
    $url = "https://github.com/$Repo/releases/download/$Version/$asset"
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("flameframe-install-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tempRoot | Out-Null
try {
    $archive = Join-Path $tempRoot $asset
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archive
    Expand-Archive -Path $archive -DestinationPath $tempRoot -Force

    $binary = Join-Path $tempRoot "flameframe-$target\flameframe.exe"
    if (-not (Test-Path $binary)) {
        throw "Release asset did not contain flameframe.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $installed = Join-Path $InstallDir "flameframe.exe"
    Copy-Item $binary $installed -Force

    if (-not $NoPathUpdate) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $pathEntries = @($userPath -split ";" | Where-Object { $_ })
        if ($pathEntries -notcontains $InstallDir) {
            $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            $env:Path = "$env:Path;$InstallDir"
            Write-Host "Added $InstallDir to the user PATH. Open a new terminal if your shell does not see it."
        }
    }

    Write-Host "Installed: $installed"
    & $installed --version
    Write-Host "Next: install ffmpeg/ffprobe and yt-dlp, then run: flameframe doctor"
} finally {
    Remove-Item $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
