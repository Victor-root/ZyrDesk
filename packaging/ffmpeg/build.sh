#!/usr/bin/env bash
#
# Builds the reduced FFmpeg that the MZ engine loads at runtime.
#
# Only what the engine uses is compiled in: H.264, HEVC and AV1
# decoding, Opus both ways, x264 as the software encoder and, on
# Windows, the hardware encoders of the three GPU vendors, Media
# Foundation and Direct3D 11 decoding. The engine opens the libraries
# itself at runtime, so nothing here takes part in the Rust build.
#
# Usage:
#   build.sh windows <out-dir>   cross-compiles the DLLs kept in vendor/ffmpeg
#   build.sh linux <out-dir>     native build of the same codecs, for tests
#
# Runs on Linux (Ubuntu 24.04 is what it is written against). Needs
# curl, git, tar, xz, make, gcc, nasm and pkg-config; the windows target
# also needs cmake and the mingw-w64 cross toolchain. On Ubuntu:
#   apt install mingw-w64 nasm cmake pkg-config make gcc git curl xz-utils
#
# Every source is pinned: a tarball by its SHA-256, a git repository by
# the exact commit of its release. Nothing is taken from the system
# besides the toolchain, so both targets carry the same code.

set -euo pipefail

ffmpeg_version=9.0.2
ffmpeg_sha256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e

# Head of x264's stable branch; x264 publishes no release tarballs.
x264_commit=b35605ace3ddf7c1a5d67a2eb553f034aef41d55

opus_version=1.6.1
opus_sha256=6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1

nv_codec_headers_tag=n13.1.15.0
nv_codec_headers_commit=0a6fba9a2820628b8103464f4c8753ee05838baa

amf_version=1.5.2
amf_headers_sha256=d3c12eb324edf05e214608b6a395a51dd95770ed9d45520185d6c3a206811c99
# The headers archive carries no licence file; this is the one of the
# repository at the commit the release was made from.
amf_commit=eadd00804d5f7e5cd8c85d540073198312870776
amf_license_sha256=eb297397aaa455b5668ab67d216b83828466152dab123fa92384c6ec16b74170

libvpl_tag=v2.17.0
libvpl_commit=d77f9195cf495b937631607333288fd917ae8939

mingw_host=x86_64-w64-mingw32

# The only DLLs the Windows build may import besides its own: all of
# them ship with every Windows. A toolchain runtime among the imports
# (libgcc, libstdc++, libwinpthread, libssp) would be missing on the
# user's machine, so the build stops instead of producing it.
windows_system_dlls=(ADVAPI32.dll bcrypt.dll KERNEL32.dll msvcrt.dll ole32.dll)

usage() {
    echo "usage: $0 windows|linux <out-dir>" >&2
    exit 2
}

[[ $# -eq 2 ]] || usage
target="$1"
case "${target}" in
    windows | linux) ;;
    *) usage ;;
esac
out_dir="$(realpath -m "$2")"

required_tools=(curl git tar xz make gcc nasm pkg-config sha256sum)
if [[ "${target}" == windows ]]; then
    required_tools+=(cmake "${mingw_host}-gcc" "${mingw_host}-g++" "${mingw_host}-objdump")
fi
for tool in "${required_tools[@]}"; do
    if ! command -v "${tool}" > /dev/null; then
        echo "missing tool: ${tool}" >&2
        exit 1
    fi
done

jobs="$(nproc)"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/zyrdesk-ffmpeg.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT
src_dir="${work_dir}/src"
deps_dir="${work_dir}/deps"
mkdir -p "${src_dir}" "${deps_dir}"

# Only the dependencies built here are visible to pkg-config, never the
# ones installed on the machine.
export PKG_CONFIG_LIBDIR="${deps_dir}/lib/pkgconfig"
unset PKG_CONFIG_PATH

fetch_file() {
    local url="$1" sha256="$2" file="$3"
    curl -sSfL --retry 3 -o "${file}" "${url}"
    echo "${sha256}  ${file}" | sha256sum -c --quiet -
}

fetch_tarball() {
    local url="$1" sha256="$2"
    local archive="${work_dir}/${url##*/}"
    fetch_file "${url}" "${sha256}" "${archive}"
    tar -xf "${archive}" -C "${src_dir}"
    rm "${archive}"
}

# A tag can be moved after the fact; the commit it must point to cannot.
fetch_git() {
    local url="$1" ref="$2" commit="$3" dir="$4"
    git init -q "${dir}"
    git -C "${dir}" fetch -q --depth 1 "${url}" "${ref}"
    local fetched
    fetched="$(git -C "${dir}" rev-parse 'FETCH_HEAD^{commit}')"
    if [[ "${fetched}" != "${commit}" ]]; then
        echo "${url} ${ref} is ${fetched}, expected ${commit}" >&2
        exit 1
    fi
    git -C "${dir}" -c advice.detachedHead=false checkout -q FETCH_HEAD
}

echo "Fetching sources"
fetch_tarball "https://ffmpeg.org/releases/ffmpeg-${ffmpeg_version}.tar.xz" "${ffmpeg_sha256}"
fetch_tarball "https://downloads.xiph.org/releases/opus/opus-${opus_version}.tar.gz" "${opus_sha256}"
fetch_git https://code.videolan.org/videolan/x264.git "${x264_commit}" "${x264_commit}" "${src_dir}/x264"
if [[ "${target}" == windows ]]; then
    fetch_git https://github.com/FFmpeg/nv-codec-headers.git "refs/tags/${nv_codec_headers_tag}" \
        "${nv_codec_headers_commit}" "${src_dir}/nv-codec-headers"
    fetch_git https://github.com/intel/libvpl.git "refs/tags/${libvpl_tag}" "${libvpl_commit}" "${src_dir}/libvpl"
    fetch_tarball "https://github.com/GPUOpen-LibrariesAndSDKs/AMF/releases/download/v${amf_version}/AMF-headers-v${amf_version}.tar.gz" \
        "${amf_headers_sha256}"
    fetch_file "https://raw.githubusercontent.com/GPUOpen-LibrariesAndSDKs/AMF/${amf_commit}/LICENSE.txt" \
        "${amf_license_sha256}" "${src_dir}/AMF-LICENSE.txt"
fi

if [[ "${target}" == windows ]]; then
    x264_target_options=(--host="${mingw_host}" --cross-prefix="${mingw_host}-")
    opus_target_options=(--host="${mingw_host}")
else
    # Linked into shared libraries, so they have to be position
    # independent.
    x264_target_options=(--enable-pic)
    opus_target_options=(--with-pic)
fi

echo "x264"
# Threads default to the native Windows ones there, which is what keeps
# libwinpthread out. Eight bits only: ten-bit H.264 has no hardware
# decoder to play it back.
(
    cd "${src_dir}/x264"
    ./configure --prefix="${deps_dir}" "${x264_target_options[@]}" \
        --enable-static --disable-cli --disable-opencl --bit-depth=8
    make -j"${jobs}"
    make install-lib-static
)

echo "Opus"
(
    cd "${src_dir}/opus-${opus_version}"
    ./configure --prefix="${deps_dir}" "${opus_target_options[@]}" \
        --disable-shared --enable-static --disable-doc --disable-extra-programs
    make -j"${jobs}"
    make install
)

if [[ "${target}" == windows ]]; then
    echo "NVIDIA codec headers"
    make -C "${src_dir}/nv-codec-headers" PREFIX="${deps_dir}" install

    echo "AMD AMF headers"
    mkdir -p "${deps_dir}/include"
    cp -r "${src_dir}/amf-headers-v${amf_version}/AMF" "${deps_dir}/include/"

    echo "Intel VPL dispatcher"
    cmake \
        -S "${src_dir}/libvpl" \
        -B "${work_dir}/build-libvpl" \
        -DCMAKE_SYSTEM_NAME=Windows \
        -DCMAKE_SYSTEM_PROCESSOR=x86_64 \
        -DCMAKE_C_COMPILER="${mingw_host}-gcc" \
        -DCMAKE_CXX_COMPILER="${mingw_host}-g++" \
        -DCMAKE_RC_COMPILER="${mingw_host}-windres" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="${deps_dir}" \
        -DBUILD_SHARED_LIBS=OFF \
        -DBUILD_TESTS=OFF \
        -DBUILD_EXAMPLES=OFF \
        -DINSTALL_EXAMPLES=OFF
    cmake --build "${work_dir}/build-libvpl" --parallel "${jobs}"
    cmake --install "${work_dir}/build-libvpl"
fi

ffmpeg_options=(
    --pkg-config=pkg-config
    --pkg-config-flags=--static
    --enable-gpl
    --enable-shared
    --disable-static
    --disable-programs
    --disable-doc
    --disable-debug
    --disable-autodetect
    --disable-everything
    --disable-avformat
    --disable-avdevice
    --disable-avfilter
    --disable-swscale
    --disable-network
    --enable-libx264
    --enable-libopus
    --enable-decoder=h264,hevc,av1,opus
    --enable-parser=h264,hevc,av1,opus
    --enable-encoder=libx264,libopus
)

if [[ "${target}" == windows ]]; then
    ffmpeg_options+=(
        --target-os=mingw32
        --arch=x86_64
        --cross-prefix="${mingw_host}-"
        # The AMF headers come without a pkg-config file.
        --extra-cflags="-I${deps_dir}/include"
        # Every runtime of the toolchain goes inside the DLLs; the Intel
        # dispatcher is C++, and nothing else names its runtime.
        --extra-ldflags=-static
        --extra-libs=-lstdc++
        --enable-w32threads
        --enable-d3d11va
        --enable-ffnvcodec
        --enable-nvenc
        --enable-amf
        --enable-libvpl
        --enable-mediafoundation
        --enable-encoder=h264_nvenc,hevc_nvenc,av1_nvenc,h264_amf,hevc_amf,av1_amf,h264_qsv,hevc_qsv,av1_qsv,h264_mf,hevc_mf
        --enable-hwaccel=h264_d3d11va,h264_d3d11va2,hevc_d3d11va,hevc_d3d11va2,av1_d3d11va,av1_d3d11va2
    )
else
    ffmpeg_options+=(
        --prefix="${out_dir}"
        # The libraries find each other in that folder, as the DLLs do
        # next to each other on Windows, without touching the loader's
        # search path.
        --enable-rpath
        --enable-pthreads
    )
fi

echo "FFmpeg"
ffmpeg_dir="${src_dir}/ffmpeg-${ffmpeg_version}"
stage_dir="${work_dir}/stage"
(
    cd "${ffmpeg_dir}"
    ./configure "${ffmpeg_options[@]}"
    make -j"${jobs}"
    # Installing strips the libraries. On Windows they are staged under
    # the default prefix, which keeps build paths out of the
    # configuration the libraries report.
    if [[ "${target}" == windows ]]; then
        make install DESTDIR="${stage_dir}"
    else
        make install
    fi
)

if [[ "${target}" == linux ]]; then
    echo "Linux build in ${out_dir}"
    exit 0
fi

echo "Checking imports"
dlls=("${stage_dir}"/usr/local/bin/*.dll)
for dll in "${dlls[@]}"; do
    while read -r imported; do
        allowed=no
        for name in "${windows_system_dlls[@]}" "${dlls[@]##*/}"; do
            if [[ "${imported,,}" == "${name,,}" ]]; then
                allowed=yes
            fi
        done
        if [[ "${allowed}" == no ]]; then
            echo "${dll##*/} imports ${imported}, which Windows does not provide" >&2
            exit 1
        fi
    done < <("${mingw_host}-objdump" -p "${dll}" | sed -n 's/^\s*DLL Name: //p')
done

echo "Copying to ${out_dir}"
mkdir -p "${out_dir}"
rm -f "${out_dir}"/*.dll
rm -rf "${out_dir}/licenses"
cp "${dlls[@]}" "${out_dir}/"

# The build is GPL version 2 or later, as configure reports, because of
# x264; the other licences are the notices their authors ask to carry.
licenses_dir="${out_dir}/licenses"
mkdir -p "${licenses_dir}"
cp "${ffmpeg_dir}/LICENSE.md" "${licenses_dir}/FFmpeg-LICENSE.md"
cp "${ffmpeg_dir}/COPYING.GPLv2" "${licenses_dir}/FFmpeg-COPYING.GPLv2"
cp "${src_dir}/x264/COPYING" "${licenses_dir}/x264-COPYING"
cp "${src_dir}/opus-${opus_version}/COPYING" "${licenses_dir}/opus-COPYING"
cp "${src_dir}/libvpl/LICENSE" "${licenses_dir}/libvpl-LICENSE"
cp "${src_dir}/AMF-LICENSE.txt" "${licenses_dir}/AMF-LICENSE.txt"
# Each of these headers carries its own notice and there is no licence
# file beside them.
for header in "${src_dir}/nv-codec-headers/include/ffnvcodec/"*.h; do
    echo "${header##*/}"
    echo
    sed '/\*\//q' "${header}"
    echo
done > "${licenses_dir}/nv-codec-headers-LICENSE.txt"

echo "Windows build in ${out_dir}"
