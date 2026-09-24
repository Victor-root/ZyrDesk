# Puts the engines built by the CI in place, without building them here.
#
# Neither engine is compiled on the machine that uses it: one wants
# MSYS2 and a GCC toolchain, the other Qt and Visual Studio, and both
# take the better part of an hour. The CI builds them once when their
# pinned source moves; this brings the result over.
#
# It goes through the GitHub CLI rather than a plain download, and that
# is not about the repository being open or closed. What a workflow
# leaves behind is served to somebody who has signed in and to nobody
# else, whatever the repository's own visibility: a plain download of an
# artifact answers with a refusal even on a repository anybody can read.
# `gh` already holds the credentials, so no token has to be written down
# anywhere on the machine.
#
# What would do away with it is publishing the engines as a release
# rather than as what a build leaves behind: release files are served to
# anybody on an open repository, and would want neither the program nor
# a sign-in.
#
# Nothing is fetched when the engines already come from the latest
# build: they weigh tens of megabytes, and they move a handful of times
# over the life of the project.

[CmdletBinding()]
param(
    # Where the engines are expected, exactly as the product looks for
    # them.
    [string] $Engines = (Join-Path $PSScriptRoot "..\..\data\engines"),
    # Repository whose builds are picked up.
    [string] $Repository = "Victor-root/ZyrDesk",
    # Fetches again even when the engines already come from that build.
    [switch] $Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# The workflow is named by its file: its display name is written for
# people, and may change without anything breaking.
$workflow = "moteurs.yml"

# What each artifact is called, and where the product expects it.
$parts = @(
    @{ Artifact = "zyrdesk-host-engine"; Folder = "host"; Named = "Moteur hôte" }
    @{ Artifact = "zyrdesk-client-engine"; Folder = "client"; Named = "Moteur client" }
)

$marker = Join-Path $Engines "build.txt"

function Assert-Ran($what) {
    if ($LASTEXITCODE -ne 0) {
        throw "$what a échoué (code $LASTEXITCODE)"
    }
}

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Write-Host "Le programme « gh » est introuvable." -ForegroundColor Yellow
    Write-Host "  Il sert à récupérer les moteurs compilés. Ce que laisse une compilation"
    Write-Host "  n'est servi qu'à quelqu'un d'identifié, même sur un dépôt ouvert."
    Write-Host "  À installer une seule fois :"
    Write-Host ""
    Write-Host "      winget install --id GitHub.cli"
    Write-Host "      gh auth login"
    Write-Host ""
    exit 1
}

Write-Host "Recherche de la dernière compilation des moteurs..."
$found = gh run list --repo $Repository --workflow $workflow --status success `
    --limit 1 --json databaseId,headSha,headBranch,createdAt
Assert-Ran "la consultation des compilations"

# Gathered before reading: a program's output arrives line by line, and
# JSON read line by line is no longer JSON.
$runs = @(($found -join "`n") | ConvertFrom-Json)
if ($runs.Count -eq 0) {
    Write-Host "Aucune compilation des moteurs n'a encore abouti." -ForegroundColor Yellow
    Write-Host "  Elle se déclenche quand la version épinglée d'un moteur change,"
    Write-Host "  et se relance à la main depuis l'onglet Actions, workflow « Moteurs »."
    exit 1
}

$run = $runs[0]
$id = $run.databaseId
$stamp = @(
    "# Moteurs ZyrDesk : d'où viennent ceux qui sont en place.",
    "# Écrit par packaging/engines/fetch-engines.ps1, à ne pas corriger à la main.",
    "run = $id",
    "commit = $($run.headSha)",
    "branche = $($run.headBranch)",
    "date = $($run.createdAt)"
) -join "`n"

if ((-not $Force) -and (Test-Path $marker)) {
    if ((Get-Content $marker -Raw).Trim() -eq $stamp.Trim()) {
        Write-Host "Moteurs déjà à jour (compilation $id du $($run.createdAt))."
        exit 0
    }
}

foreach ($part in $parts) {
    $folder = Join-Path $Engines $part.Folder
    Write-Host "$($part.Named) : récupération..."

    # Emptied rather than written over: a file the build no longer
    # produces would stay behind for good, and a mixture of two builds
    # fails in ways nobody can read.
    try {
        if (Test-Path $folder) {
            Remove-Item $folder -Recurse -Force
        }
    }
    catch {
        throw "$($part.Named) est en cours d'utilisation. Arrêtez le service et fermez ZyrDesk, puis recommencez.`n  $_"
    }
    New-Item -ItemType Directory -Path $folder -Force | Out-Null

    gh run download $id --repo $Repository --name $part.Artifact --dir $folder
    Assert-Ran "la récupération de $($part.Named)"
}

Set-Content -Path $marker -Value $stamp -Encoding UTF8
Write-Host ""
Write-Host "Moteurs en place, compilation $id du $($run.createdAt)." -ForegroundColor Green
