#!/usr/bin/env bash
#
# Builds the ZyrDesk host engine from the pinned Sunshine source.
#
# The engine's own build is not used whole: it also produces two Windows
# installers, builds its documentation and its test suite, and signs
# what it makes, none of which we want. What is kept here is the subset
# that matters, configured with the same options in the same order, so
# the two stay comparable when upstream moves.
#
# Nothing ZyrDesk-specific lives in the engine itself: the name, the
# icon and the publisher are handed in at configure time, through
# switches the engine exposes for exactly that.
#
# Run from an MSYS2 UCRT64 shell with the engine's dependencies already
# installed. The workflow next to this file does that.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_dir="$(cd "${1:-${here}/../../engines/sunshine}" && pwd)"
output_dir="$(realpath -m "${2:-${here}/../../data/engines/host}")"

build_dir="${source_dir}/build-zyrdesk"
brand_dir="$(cd "${here}/../brand" && pwd)"

echo "Moteur hôte : source ${source_dir}"

# The engine carries its dependencies as submodules, including the
# prebuilt Windows libraries. A missing one fails deep inside the
# compiler with nothing readable, so it is worth making sure first.
git -C "${source_dir}" submodule update --init --recursive

rm -rf "${build_dir}"

echo "Configuration"
# The name, the icon and the publisher are the whole of the rebranding:
# they are what the task manager and the file properties dialog show.
# The version is left to the engine, which reports its own commit when
# no release number is handed to it; it is written nowhere the user
# looks, and the protocol version it announces is a separate constant.
#
# Configuring downloads a gamepad driver installer the engine packages
# but that we never ship: that step belongs to its own installer and
# happens whether or not it is asked for.
cmake \
    -B "${build_dir}" \
    -G Ninja \
    -S "${source_dir}" \
    -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    -DBUILD_DOCS=OFF \
    -DBUILD_TESTS=OFF \
    -DSUNSHINE_ASSETS_DIR=assets \
    -DSUNSHINE_PRODUCT_NAME="ZyrDesk : Moteur de diffusion" \
    -DSUNSHINE_ICON_PATH="$(cygpath -m "${brand_dir}/zyrdesk.ico")" \
    -DSUNSHINE_PUBLISHER_NAME=ZyrDesk \
    -DSUNSHINE_PUBLISHER_WEBSITE="https://github.com/Victor-root/ZyrDesk" \
    -DSUNSHINE_PUBLISHER_ISSUE_URL="https://github.com/Victor-root/ZyrDesk/issues"

echo "Compilation"
# Named rather than built whole: the rest of what the engine produces is
# its service wrapper, which our own service replaces, and two command
# line tools we never call.
ninja -C "${build_dir}" sunshine web-ui

echo "Assemblage"
rm -rf "${output_dir}"
mkdir -p "${output_dir}/assets"

# The compiler names it after the project, and the project is still
# theirs; what the user reads is inside the file, and the patch lets our
# build set it. The file itself is named here.
cp "${build_dir}/sunshine.exe" "${output_dir}/zyrdesk-host-engine.exe"

# Everything else on Windows is linked in, except this one, which
# OpenSSL loads by name at runtime.
zlib="$(sed -n 's/^ZLIB:FILEPATH=//p' "${build_dir}/CMakeCache.txt")"
if [[ -z "${zlib}" || ! -f "${zlib}" ]]; then
    echo "bibliothèque zlib introuvable : le moteur ne démarrerait pas" >&2
    exit 1
fi
cp "${zlib}" "${output_dir}/"

# The engine resolves these against the folder it is launched from, and
# our supervisor launches it from its own folder. Taken from the source
# rather than from the build folder, where the shaders are a junction.
# The web interface is the one exception: what sits in the source is the
# material a separate tool turns into the served files.
cp -r "${source_dir}/src_assets/common/assets/." "${output_dir}/assets/"
cp -r "${source_dir}/src_assets/windows/assets/." "${output_dir}/assets/"
rm -rf "${output_dir}/assets/web"
cp -r "${build_dir}/assets/web" "${output_dir}/assets/"

# What breaks a packaged engine is almost always a library left behind,
# and it only shows on someone else's machine, at the first launch.
# Starting the engine here would prove nothing: it wants a screen to
# capture and a session to run in. Its dependencies can be read without
# running it.
echo "Vérification des dépendances"
missing="$(ldd "${output_dir}/zyrdesk-host-engine.exe" | awk '
    BEGIN { IGNORECASE = 1 }
    # Names in api-ms-win and ext-ms-win are not files at all: Windows
    # resolves them itself to whatever carries them on that version.
    $1 ~ /^(api|ext)-ms-win-/ { next }
    $1 == "zlib1.dll" { next }
    # Anything still answered from the build toolchain would be missing
    # on a machine that does not have it, which is every machine but
    # this one.
    $0 ~ /=>[[:space:]]+not found/ { print $1 }
    $0 ~ /=>[[:space:]]+\/(clangarm64|clang64|mingw32|mingw64|ucrt64)\/bin\// { print $1 }
')"
if [[ -n "${missing}" ]]; then
    echo "bibliothèques manquantes à côté du moteur :" >&2
    echo "${missing}" >&2
    exit 1
fi

# The web interface is built by a separate tool during the same run, and
# it fails in a way the compiler never reports.
if [[ ! -f "${output_dir}/assets/web/index.html" ]]; then
    echo "interface web du moteur absente : son démarrage échouerait" >&2
    exit 1
fi

echo "Moteur hôte assemblé dans ${output_dir}"
