# Builds the ZyrDesk client engine from the pinned Moonlight source.
#
# The engine's own build script is not used: it also produces an MSI
# installer, publishes debugging symbols and signs binaries, none of
# which we want, and it drags in the WiX toolset for an artifact we
# would throw away. What is kept here is the subset that matters, in
# the same order and with the same options, so the two stay comparable
# when upstream moves.
#
# Nothing ZyrDesk-specific lives in the engine itself: this script and
# the layout it produces are the product's, the source stays upstream's.
#
# Run from a shell where the Visual Studio environment and Qt's bin
# directory are already on PATH.

[CmdletBinding()]
param(
    # Where the engine source sits.
    [string] $Source = (Join-Path $PSScriptRoot "..\..\engines\moonlight-qt"),
    # Where the finished engine is assembled.
    [string] $Output = (Join-Path $PSScriptRoot "..\..\data\engines\client"),
    # Kept as a parameter so a developer can build a debug engine when
    # chasing something; the product always ships release.
    [ValidateSet("release", "debug")]
    [string] $Configuration = "release"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Source = (Resolve-Path $Source).Path
$build = Join-Path $Source "build-zyrdesk"

function Assert-Ran($what) {
    if ($LASTEXITCODE -ne 0) {
        throw "$what a échoué (code $LASTEXITCODE)"
    }
}

Write-Host "Moteur client : source $Source"

# The engine carries its own dependencies as submodules, including the
# prebuilt Windows libraries. A missing one fails deep inside the
# compiler with nothing readable, so it is worth making sure first.
Push-Location $Source
try {
    git submodule update --init --recursive
    Assert-Ran "la récupération des sous-modules du moteur"
} finally {
    Pop-Location
}

if (Test-Path $build) {
    Remove-Item -Recurse -Force $build
}
New-Item -ItemType Directory -Path $build | Out-Null

Write-Host "Configuration"
Push-Location $build
try {
    qmake.exe (Join-Path $Source "moonlight-qt.pro")
    Assert-Ran "la configuration du moteur"

    Write-Host "Compilation"
    # jom is the parallel make the engine ships with; nmake would build
    # on a single core and take several times as long.
    & (Join-Path $Source "scripts\jom.exe") $Configuration
    Assert-Ran "la compilation du moteur"
} finally {
    Pop-Location
}

if (Test-Path $Output) {
    Remove-Item -Recurse -Force $Output
}
New-Item -ItemType Directory -Path $Output -Force | Out-Null

Write-Host "Assemblage"
Copy-Item (Join-Path $Source "libs\windows\lib\x64\*.dll") $Output
Copy-Item (Join-Path $build "AntiHooking\$Configuration\AntiHooking.dll") $Output
Copy-Item (Join-Path $Source "app\SDL_GameControllerDB\gamecontrollerdb.txt") $Output

# Qt's own deployment tool, with the engine's option set: the styles and
# tools left out here are the ones it never loads, and they weigh more
# than everything else put together.
$qtArguments = @(
    "--dir", $Output
    "--$Configuration"
    "--qmldir", (Join-Path $Source "app\gui")
    "--no-opengl-sw", "--no-compiler-runtime", "--no-sql"
    "--no-system-d3d-compiler", "--no-system-dxc-compiler"
    "--skip-plugin-types", "qmltooling,generic"
    "--no-ffmpeg"
    "--no-quickcontrols2fusion", "--no-quickcontrols2imagine", "--no-quickcontrols2universal"
    "--no-quickcontrols2fusionstyleimpl", "--no-quickcontrols2imaginestyleimpl"
    "--no-quickcontrols2universalstyleimpl", "--no-quickcontrols2windowsstyleimpl"
)
windeployqt.exe @qtArguments (Join-Path $build "app\$Configuration\Moonlight.exe")
Assert-Ran "le déploiement des dépendances Qt"

foreach ($unused in @(
        "qml\QtQuick\Controls\Fusion", "qml\QtQuick\Controls\Imagine",
        "qml\QtQuick\Controls\Universal", "qml\QtQuick\Controls\Windows",
        "qml\QtQuick\NativeStyle")) {
    $path = Join-Path $Output $unused
    if (Test-Path $path) {
        Remove-Item -Recurse -Force $path
    }
}

# The name the product shows everywhere, and the one the service and the
# command line look for. What is inside the file still says otherwise
# until the rebranding patch lands: renaming is not rebranding.
Copy-Item (Join-Path $build "app\$Configuration\Moonlight.exe") (Join-Path $Output "zyrdesk-session.exe")

# The engine keeps its settings next to itself when this file is there,
# instead of in the registry. That is what keeps a ZyrDesk install from
# touching the settings of a Moonlight the user may already have.
New-Item -ItemType File -Path (Join-Path $Output "portable.dat") -Force | Out-Null

# The C++ runtime the engine was built against. Without it, the engine
# starts only on machines that happen to have the right version already.
$vswhere = Join-Path $Source "scripts\vswhere.exe"
$redist = & $vswhere -latest -find "VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT" | Select-Object -Last 1
if (-not $redist) {
    throw "runtime C++ introuvable : le moteur ne démarrerait que sur cette machine"
}
Copy-Item (Join-Path $redist "*.dll") $Output

# What breaks a packaged engine is almost always a library left behind,
# and it only shows on someone else's machine, at the first launch.
# Starting the engine here would prove nothing: it draws a window, and
# a build machine has no screen to draw it on. Its dependencies can be
# read without running it.
Write-Host "Vérification des dépendances"

$engine = Join-Path $Output "zyrdesk-session.exe"
$reading = & dumpbin.exe /dependents $engine
Assert-Ran "la lecture des dépendances du moteur"

$missing = @()
foreach ($line in $reading) {
    if ($line -notmatch '^\s+(\S+\.dll)\s*$') { continue }
    $library = $Matches[1]
    $besideIt = Test-Path (Join-Path $Output $library)
    $fromWindows = Test-Path (Join-Path $env:SystemRoot "System32\$library")
    if (-not $besideIt -and -not $fromWindows) {
        $missing += $library
    }
}
if ($missing.Count -gt 0) {
    throw "bibliothèques manquantes à côté du moteur : $($missing -join ', ')"
}

# Qt loads this one by name at startup rather than declaring it, so it
# is invisible to the check above. Without it a Qt program stops before
# showing anything, with a message naming no cause.
$platform = Join-Path $Output "platforms\qwindows.dll"
if (-not (Test-Path $platform)) {
    throw "greffon de plateforme Qt absent : le moteur ne s'ouvrirait pas"
}

Write-Host "Moteur client assemblé dans $Output"
