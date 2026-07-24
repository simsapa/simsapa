# Windows build script for Simsapa
# This script builds the Qt6 application, deploys dependencies with windeployqt,
# and creates an installer with Inno Setup

param(
    [string]$AppName = "",
    [string]$AppVersion = "",
    [string]$QtPath = "C:\Qt\6.9.3\msvc2022_64",
    [string]$BuildDir = ".\build\simsapadhammareader",
    [string]$DistDir = ".\dist",
    [switch]$Clean,
    [switch]$SkipBuild,
    [switch]$SkipDeploy,
    [switch]$SkipInstaller,
    [switch]$Verbose,
    [switch]$Help
)

# Function to show usage
function Show-Usage {
    Write-Host @"
Usage: .\build-windows.ps1 [OPTIONS]

Options:
  -AppName NAME         Set application name (default: read from .desktop file)
  -AppVersion VER       Set application version (default: read from Cargo.toml)
  -QtPath PATH          Set Qt installation path (default: C:\Qt\6.9.3\msvc2022_64)
  -BuildDir PATH        Set build directory (default: .\build\simsapadhammareader)
  -DistDir PATH         Set distribution directory (default: .\dist)
  -Clean                Clean build artifacts before building
  -SkipBuild            Skip building, use existing executable
  -SkipDeploy           Skip Qt deployment
  -SkipInstaller        Skip installer creation
  -Verbose              Show detailed debugging output
  -Help                 Show this help message

"@
}

# Colors for output
function Write-Status {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warning {
    param([string]$Message)
    Write-Host "[WARNING] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# Function to extract app name from desktop file
function Get-AppNameFromDesktop {
    if (Test-Path "simsapa.desktop") {
        $content = Get-Content "simsapa.desktop"
        foreach ($line in $content) {
            if ($line -match "^Name=(.+)$") {
                return $matches[1].Trim()
            }
        }
    }
    return "Simsapa"
}

# Function to extract version from Cargo.toml
function Get-VersionFromCargoToml {
    if (Test-Path "bridges\Cargo.toml") {
        $content = Get-Content "bridges\Cargo.toml"
        foreach ($line in $content) {
            if ($line -match '^version\s*=\s*"([^"]+)"') {
                return $matches[1]
            }
        }
    }
    return "0.1.8"
}

# Show help if requested
if ($Help) {
    Show-Usage
    exit 0
}

# Set defaults if not provided
if ([string]::IsNullOrEmpty($AppName)) {
    $AppName = Get-AppNameFromDesktop
}
if ([string]::IsNullOrEmpty($AppVersion)) {
    $AppVersion = Get-VersionFromCargoToml
}

Write-Status "Building Windows installer for Simsapa..."
Write-Status "  App Name: $AppName"
Write-Status "  App Version: $AppVersion"
Write-Status "  Qt Path: $QtPath"
Write-Status "  Build Dir: $BuildDir"
Write-Status "  Dist Dir: $DistDir"
Write-Status ""

# Check if Qt installation exists
if (-not (Test-Path $QtPath)) {
    Write-Error "Qt installation not found at: $QtPath"
    Write-Error "Please install Qt 6.9.3 or specify the correct path with -QtPath"
    exit 1
}

# Find required tools
$qtBinPath = Join-Path $QtPath "bin"
$windeployqt = Join-Path $qtBinPath "windeployqt.exe"

# Find Qt Tools directory (where CMake and Ninja are installed)
# Qt Path: C:\Qt\6.9.3\msvc2022_64
# Tools Dir: C:\Qt\Tools
$qtRootDir = Split-Path (Split-Path $QtPath -Parent) -Parent
$qtToolsDir = Join-Path $qtRootDir "Tools"
$cmake = Join-Path $qtToolsDir "CMake_64\bin\cmake.exe"
$ninja = Join-Path $qtToolsDir "Ninja\ninja.exe"

if ($Verbose) {
    Write-Status "Debug: Qt Path: $QtPath"
    Write-Status "Debug: Qt Root Dir: $qtRootDir"
    Write-Status "Debug: Qt Tools Dir: $qtToolsDir"
    Write-Status "Debug: Expected CMake: $cmake"
    Write-Status "Debug: Expected Ninja: $ninja"
}

# Add Qt tools to PATH so CMake can find them
$qtCMakeBinDir = Join-Path $qtToolsDir "CMake_64\bin"
$qtNinjaDir = Join-Path $qtToolsDir "Ninja"

if (Test-Path $qtCMakeBinDir) {
    $env:PATH = "$qtCMakeBinDir;$env:PATH"
    Write-Status "[OK] Added Qt CMake to PATH: $qtCMakeBinDir"
} else {
    Write-Warning "Qt CMake directory not found: $qtCMakeBinDir"
}

if (Test-Path $qtNinjaDir) {
    $env:PATH = "$qtNinjaDir;$env:PATH"
    Write-Status "[OK] Added Qt Ninja to PATH: $qtNinjaDir"
} else {
    Write-Warning "Qt Ninja directory not found: $qtNinjaDir"
}

# Check for CMake
if (-not (Test-Path $cmake)) {
    Write-Warning "CMake not found at: $cmake"
    Write-Status "Trying to find CMake in system PATH..."
    $cmake = "cmake"
    if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
        Write-Error "CMake not found. Please install CMake or ensure Qt Tools is installed"
        exit 1
    }
} else {
    Write-Status "[OK] Found CMake: $cmake"
}

# Check for Ninja
if (-not (Test-Path $ninja)) {
    Write-Warning "Ninja not found at: $ninja"
    Write-Status "Trying to find Ninja in system PATH..."
    $ninja = "ninja"
    if (-not (Get-Command ninja -ErrorAction SilentlyContinue)) {
        Write-Warning "Ninja not found, CMake will use Visual Studio generator instead"
        $ninja = $null
    }
} else {
    Write-Status "[OK] Found Ninja: $ninja"
}

# Check for windeployqt
if (-not (Test-Path $windeployqt)) {
    Write-Error "windeployqt.exe not found at: $windeployqt"
    Write-Error "Please ensure Qt 6.9.3 is properly installed"
    exit 1
} else {
    Write-Status "[OK] Found windeployqt: $windeployqt"
}

Write-Status ""

# Check for Rust toolchain
try {
    # Check if any MSVC toolchain is installed (stable, beta, or nightly)
    $rustupOutput = & rustup toolchain list 2>&1
    if ($rustupOutput -match "msvc") {
        Write-Status "[OK] MSVC Rust toolchain found"
    } else {
        Write-Warning "No MSVC Rust toolchain found"
        Write-Status "Installing stable MSVC toolchain..."
        & rustup toolchain install stable-x86_64-pc-windows-msvc
    }
    
    # Also ensure the MSVC target is available for cross-compilation
    $targetOutput = & rustup target list --installed 2>&1
    if ($targetOutput -notmatch "x86_64-pc-windows-msvc") {
        Write-Status "Adding x86_64-pc-windows-msvc target..."
        & rustup target add x86_64-pc-windows-msvc
    }
} catch {
    Write-Error "Rust toolchain not found. Please install Rust from https://rustup.rs/"
    exit 1
}

# Clean if requested
if ($Clean) {
    Write-Status "Cleaning build artifacts..."
    if (Test-Path $BuildDir) {
        Remove-Item -Recurse -Force $BuildDir
    }
    if (Test-Path $DistDir) {
        Remove-Item -Recurse -Force $DistDir
    }
    if (Test-Path "Simsapa-Setup-*.exe") {
        Remove-Item -Force "Simsapa-Setup-*.exe"
    }
}

# Build the application
if (-not $SkipBuild) {
    Write-Status "Building application..."
    
    # Set up environment for MSVC by calling vcvars64.bat
    $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vcvarsPath = $null
    
    if (Test-Path $vsWhere) {
        $vsPath = & $vsWhere -latest -property installationPath
        $vcvarsPath = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
        
        if (Test-Path $vcvarsPath) {
            Write-Status "Found Visual Studio at: $vsPath"
            Write-Status "Setting up MSVC environment..."
            
            # Import environment variables from vcvars64.bat
            # We use cmd.exe to run vcvars64.bat and capture the environment
            $tempFile = [System.IO.Path]::GetTempFileName()
            
            # Run vcvars64.bat and output environment to temp file
            & cmd.exe /c "`"$vcvarsPath`" >nul 2>&1 & set > `"$tempFile`""
            
            # Read and parse environment variables
            if (Test-Path $tempFile) {
                $envVars = Get-Content $tempFile
                Remove-Item $tempFile
                
                foreach ($line in $envVars) {
                    if ($line -match '^([^=]+)=(.*)$') {
                        $name = $matches[1]
                        $value = $matches[2]
                        # Set ALL environment variables to ensure complete MSVC setup
                        Set-Item -Path "env:$name" -Value $value -Force
                    }
                }
                Write-Status "MSVC environment configured"
                
                # Verify required build tools are available
                $toolsFound = $true
                
                if (Get-Command cl.exe -ErrorAction SilentlyContinue) {
                    Write-Status "[OK] C++ compiler (cl.exe) found"
                } else {
                    Write-Warning "WARNING: C++ compiler (cl.exe) not found"
                    $toolsFound = $false
                }
                
                if (Get-Command link.exe -ErrorAction SilentlyContinue) {
                    Write-Status "[OK] Linker (link.exe) found"
                } else {
                    Write-Warning "WARNING: Linker (link.exe) not found"
                    $toolsFound = $false
                }
                
                if (Get-Command rc.exe -ErrorAction SilentlyContinue) {
                    Write-Status "[OK] Resource compiler (rc.exe) found"
                } else {
                    Write-Warning "WARNING: Resource compiler (rc.exe) not found"
                    $toolsFound = $false
                }
                
                if (-not $toolsFound) {
                    Write-Warning "Some build tools are missing. Build may fail."
                    Write-Warning "Please ensure you're running from Developer PowerShell for VS 2022"
                }
            } else {
                Write-Warning "Failed to capture MSVC environment"
            }
        } else {
            Write-Warning "vcvars64.bat not found at: $vcvarsPath"
        }
    }
    
    if (-not $vcvarsPath -or -not (Test-Path $vcvarsPath)) {
        Write-Warning "Visual Studio environment not configured"
        Write-Warning ""
        Write-Warning "RECOMMENDED: Run this script from 'Developer PowerShell for VS 2022'"
        Write-Warning "  1. Open Start Menu"
        Write-Warning "  2. Search for 'Developer PowerShell for VS 2022'"
        Write-Warning "  3. Navigate to: $PWD"
        Write-Warning "  4. Run: .\build-windows.ps1"
        Write-Warning ""
    }
    
    # Check if we're already in a Developer environment
    if ($env:VSCMD_VER) {
        Write-Status "[OK] Running in Visual Studio Developer environment (version $env:VSCMD_VER)"
    } elseif ($env:VisualStudioVersion) {
        Write-Status "[OK] Visual Studio environment detected (version $env:VisualStudioVersion)"
    } else {
        Write-Warning "Not running in Visual Studio Developer environment"
        Write-Warning "The build may fail due to missing Windows SDK tools"
        Write-Warning ""
        Write-Warning "SOLUTION: Use 'Developer PowerShell for VS 2022' instead of regular PowerShell"
        Write-Warning ""
    }
    
    # Configure with CMake
    Write-Status "Configuring CMake..."
    $cmakeArgs = @(
        "-S", ".",
        "-B", $BuildDir,
        "-DCMAKE_PREFIX_PATH=$QtPath",
        "-DCMAKE_BUILD_TYPE=Release"
    )
    
    # Determine which generator to use
    $useNinja = $false
    if ($ninja -and (Test-Path $ninja)) {
        # Ninja exists, tell CMake exactly where it is
        Write-Status "Using Ninja build system"
        $cmakeArgs += "-G", "Ninja"
        $cmakeArgs += "-DCMAKE_MAKE_PROGRAM=$ninja"
        $useNinja = $true
    } elseif (Get-Command ninja -ErrorAction SilentlyContinue) {
        # Ninja is in PATH
        Write-Status "Using Ninja build system (from PATH)"
        $cmakeArgs += "-G", "Ninja"
        $useNinja = $true
    } else {
        Write-Status "Using Visual Studio generator (Ninja not found)"
        # CMake will auto-detect Visual Studio
    }
    
    & $cmake $cmakeArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Error ""
        Write-Error "========================================"
        Write-Error "CMake configuration failed"
        Write-Error "========================================"
        Write-Error ""
        Write-Error "The most common cause is missing Windows SDK tools (rc.exe, mt.exe)"
        Write-Error "These tools are only available when using Developer PowerShell."
        Write-Error ""
        Write-Error "SOLUTION (Recommended):"
        Write-Error "  1. Close this PowerShell window"
        Write-Error "  2. Open Start Menu"
        Write-Error "  3. Search for: Developer PowerShell for VS 2022"
        Write-Error "  4. In Developer PowerShell, run:"
        Write-Error "       cd $PWD"
        Write-Error "       .\build-windows.ps1"
        Write-Error ""
        Write-Error "Alternative: Import VS environment in current session:"
        Write-Error "  & 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Launch-VsDevShell.ps1'"
        Write-Error "  .\build-windows.ps1"
        Write-Error ""
        exit 1
    }
    
    # Build with CMake
    Write-Status "Building with CMake..."
    & $cmake --build $BuildDir --config Release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit 1
    }
    
    Write-Status "Build completed successfully"
} else {
    Write-Status "Skipping build (using existing executable)"
}

# Check if executable exists
$exePath = Join-Path $BuildDir "simsapadhammareader.exe"
if (-not (Test-Path $exePath)) {
    Write-Error "Executable not found at: $exePath"
    Write-Error "Please build the application first or remove -SkipBuild flag"
    exit 1
}

# Deploy Qt dependencies
if (-not $SkipDeploy) {
    Write-Status "Creating distribution directory..."
    if (Test-Path $DistDir) {
        Remove-Item -Recurse -Force $DistDir
    }
    New-Item -ItemType Directory -Path $DistDir | Out-Null
    
    # Copy executable to dist directory
    Write-Status "Copying executable to distribution directory..."
    Copy-Item $exePath -Destination $DistDir
    
    # Run windeployqt
    Write-Status "Deploying Qt dependencies with windeployqt..."
    $distExePath = Join-Path $DistDir "simsapadhammareader.exe"
    
    # Add Qt bin to PATH for this session
    $env:PATH = "$qtBinPath;$env:PATH"
    
    $windeployqtArgs = @(
        $distExePath,
        "--qmldir", "assets\qml",
        "--release",
        "--no-translations",
        "--no-system-d3d-compiler",
        "--no-opengl-sw"
    )
    
    & $windeployqt $windeployqtArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "windeployqt completed with warnings (this is often normal)"
    }
    
    Write-Status "Qt dependencies deployed successfully"
    
    # Copy additional resources
    Write-Status "Copying application resources..."
    
    # Copy icon (for runtime use)
    if (Test-Path "assets\icons\appicons\simsapa.ico") {
        Copy-Item "assets\icons\appicons\simsapa.ico" -Destination $DistDir
    }
    
    Write-Status "Distribution package created at: $DistDir"
} else {
    Write-Status "Skipping Qt deployment"
}

# Download VC++ Redistributable if missing
if (-not $SkipInstaller) {
    $vcRedistDir = ".\redist"
    $vcRedistPath = "$vcRedistDir\vc_redist.x64.exe"
    $vcRedistUrl = "https://aka.ms/vs/17/release/vc_redist.x64.exe"
    
    if (-not (Test-Path $vcRedistPath)) {
        Write-Status "Downloading Visual C++ Redistributable..."
        
        # Create redist directory if it doesn't exist
        if (-not (Test-Path $vcRedistDir)) {
            New-Item -ItemType Directory -Path $vcRedistDir | Out-Null
        }
        
        try {
            # Use BITS for more reliable download with progress, fallback to Invoke-WebRequest
            $bitsSupported = Get-Command Start-BitsTransfer -ErrorAction SilentlyContinue
            if ($bitsSupported) {
                Write-Status "Downloading using BITS..."
                Start-BitsTransfer -Source $vcRedistUrl -Destination $vcRedistPath -Description "Downloading VC++ Redistributable"
            } else {
                Write-Status "Downloading using Invoke-WebRequest..."
                # Show progress for large download
                $ProgressPreference = 'Continue'
                Invoke-WebRequest -Uri $vcRedistUrl -OutFile $vcRedistPath -UseBasicParsing
            }
            
            if (Test-Path $vcRedistPath) {
                $fileSize = (Get-Item $vcRedistPath).Length / 1MB
                Write-Status "[OK] VC++ Redistributable downloaded ($([math]::Round($fileSize, 2)) MB)"
            } else {
                Write-Warning "Download completed but file not found at: $vcRedistPath"
            }
        } catch {
            Write-Warning "Failed to download VC++ Redistributable: $_"
            Write-Warning "The installer will show a warning to users if VC++ is not installed"
            Write-Warning "You can manually download from: $vcRedistUrl"
        }
    } else {
        Write-Status "[OK] VC++ Redistributable already present"
    }
}

# Create installer with Inno Setup
if (-not $SkipInstaller) {
    Write-Status "Creating installer with Inno Setup..."
    
    # Check for Inno Setup
    $innoSetupPaths = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )
    
    $iscc = $null
    foreach ($path in $innoSetupPaths) {
        if (Test-Path $path) {
            $iscc = $path
            break
        }
    }
    
    if (-not $iscc) {
        Write-Error "Inno Setup not found. Please install Inno Setup 6 from https://jrsoftware.org/isdl.php"
        Write-Warning "Installer creation skipped. You can still use the files in $DistDir"
        exit 0
    }
    
    Write-Status "Using Inno Setup at: $iscc"
    
    # Check if installer script exists
    $installerScript = "simsapa-installer.iss"
    if (-not (Test-Path $installerScript)) {
        Write-Error "Installer script not found: $installerScript"
        Write-Error "Please create the Inno Setup script first"
        exit 1
    }
    
    # Compile installer
    Write-Status "Compiling installer..."
    $installerArgs = @(
        "/DAppVersion=$AppVersion",
        "/DDistDir=$DistDir",
        $installerScript
    )
    
    & $iscc $installerArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Installer creation failed"
        exit 1
    }
    
    $installerName = "Simsapa-Setup-$AppVersion.exe"
    if (Test-Path $installerName) {
        Write-Status "Installer created successfully: $installerName"
        $installerSize = (Get-Item $installerName).Length / 1MB
        Write-Status "Installer size: $([math]::Round($installerSize, 2)) MB"

        # Generate SHA256 checksum
        Write-Status "Generating SHA256 checksum..."
        $checksumFile = "$installerName.sha256.txt"
        "$((Get-FileHash $installerName -Algorithm SHA256).Hash.ToLower())  $installerName" | Set-Content -Encoding ASCII $checksumFile
        Write-Status "Checksum written to $checksumFile"
    } else {
        Write-Warning "Installer was compiled but not found at expected location"
    }
} else {
    Write-Status "Skipping installer creation"
}

Write-Status ""
Write-Status "Windows build completed successfully!"
Write-Status "Distribution files:"
Write-Status "  - $DistDir\simsapadhammareader.exe"
if (-not $SkipInstaller) {
    Write-Status "  - Simsapa-Setup-$AppVersion.exe"
}
Write-Status ""
Write-Status "Application details (Standard install):"
Write-Status "  - Install location: C:\Program Files\Simsapa"
Write-Status "  - User data: %LOCALAPPDATA%\profound-labs\simsapa"
Write-Status "  - Databases: %LOCALAPPDATA%\profound-labs\simsapa\app-assets"
Write-Status ""
Write-Status "The same installer also offers a Portable install (user-chosen"
Write-Status "folder, sibling 'Data' folder via config.txt, no admin/uninstaller)."
Write-Status "See docs/windows-portable-install.md."
Write-Status ""

# Report VC++ Redistributable status
$vcRedistPath = ".\redist\vc_redist.x64.exe"
if (Test-Path $vcRedistPath) {
    Write-Status "VC++ Redistributable: Bundled (will install silently if needed)"
} else {
    Write-Warning "VC++ Redistributable: Not bundled (download failed or -SkipInstaller used)"
    Write-Warning "  Users will see a warning if VC++ is not installed on their system"
}
