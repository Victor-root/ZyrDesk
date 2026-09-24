#!/usr/bin/env bash
#
# Generates the FFmpeg bindings of zyr-codec from FFmpeg's own headers.
#
# Nothing here runs during the Rust build: the output is committed, and
# the script only has to be run again when FFmpeg changes or when the
# crate starts using another FFmpeg function or type.
#
# Two files come out, under src/sys:
#   bindings.rs  the types, constants and function tables of avutil,
#                swresample and avcodec. Generated once for Linux and
#                once for Windows: the two must be identical, which is
#                what lets one committed file serve both, its layout
#                checks holding on each.
#   d3d11va.rs   the two Direct3D 11 structures of hwcontext_d3d11va.h,
#                which only exist on Windows, generated for Windows
#                against the real d3d11.h so their layout checks are
#                the header's.
#
# Usage:
#   generate-bindings.sh <include-dir>
# where <include-dir> is the include folder of an FFmpeg 9.0.2 install,
# such as <out-dir>/include after `packaging/ffmpeg/build.sh linux
# <out-dir>`. The public headers do not depend on the build options.
#
# Needs bindgen 0.73.2 (`cargo install bindgen-cli --version 0.73.2`)
# and the libclang it loads, the mingw-w64 headers (Ubuntu package
# mingw-w64-x86-64-dev) and rustfmt.

set -euo pipefail

ffmpeg_version=9.0.2
bindgen_version=0.73.2
mingw_include=/usr/x86_64-w64-mingw32/include

# What the crate uses, and nothing else. A function resolved at load
# time that FFmpeg does not export stops the loading, so every name
# here must be one the three libraries really have.
avutil_functions=(
    avutil_version
    av_version_info
    av_strerror
    av_frame_alloc
    av_frame_free
    av_frame_get_buffer
    av_buffer_ref
    av_buffer_unref
    av_dict_set
    av_dict_free
    av_dict_iterate
    av_channel_layout_default
    av_channel_layout_uninit
    av_hwdevice_ctx_alloc
    av_hwdevice_ctx_init
    av_hwdevice_ctx_create_derived
    av_hwframe_ctx_alloc
    av_hwframe_ctx_init
    av_hwframe_ctx_create_derived
    av_hwframe_get_buffer
    av_hwframe_map
)
swresample_functions=(
    swresample_version
    swr_alloc_set_opts2
    swr_init
    swr_free
    swr_get_out_samples
    swr_convert
)
avcodec_functions=(
    avcodec_version
    avcodec_find_encoder_by_name
    avcodec_find_decoder_by_name
    avcodec_alloc_context3
    avcodec_free_context
    avcodec_open2
    avcodec_send_frame
    avcodec_receive_packet
    avcodec_send_packet
    avcodec_receive_frame
    av_packet_alloc
    av_packet_free
)
types=(
    AVBufferRef
    AVChannelLayout
    AVCodec
    AVCodecContext
    AVColorPrimaries
    AVColorRange
    AVColorSpace
    AVColorTransferCharacteristic
    AVDictionary
    AVDictionaryEntry
    AVFrame
    AVHWDeviceContext
    AVHWDeviceType
    AVHWFramesContext
    AVPacket
    AVPictureType
    AVPixelFormat
    AVSampleFormat
    SwrContext
)
constants=(
    FFMPEG_VERSION
    LIBAVUTIL_VERSION_MAJOR
    LIBSWRESAMPLE_VERSION_MAJOR
    LIBAVCODEC_VERSION_MAJOR
    AV_ERROR_MAX_STRING_SIZE
    AV_LOG_WARNING
    AV_FRAME_FLAG_KEY
    AV_PKT_FLAG_KEY
    AV_CODEC_FLAG_LOW_DELAY
    AV_CODEC_FLAG_CLOSED_GOP
    AV_EF_EXPLODE
    FF_THREAD_SLICE
)
d3d11va_types=(
    AVD3D11VADeviceContext
    AVD3D11VAFramesContext
    AVD3D11FrameDescriptor
)
# What those structures point to, which Rust only ever holds by
# pointer: the interfaces are opaque, the flags are a 32-bit UINT. The
# layout checks bindgen writes from the header catch any mistake here.
d3d11va_names=(
    "pub type ID3D11Device = core::ffi::c_void;"
    "pub type ID3D11DeviceContext = core::ffi::c_void;"
    "pub type ID3D11VideoDevice = core::ffi::c_void;"
    "pub type ID3D11VideoContext = core::ffi::c_void;"
    "pub type ID3D11Texture2D = core::ffi::c_void;"
    "pub type UINT = core::ffi::c_uint;"
)

targets=(x86_64-unknown-linux-gnu x86_64-pc-windows-gnu)

usage() {
    echo "usage: $0 <ffmpeg-include-dir>" >&2
    exit 2
}

[[ $# -eq 1 ]] || usage
include_dir="$(realpath "$1")"
crate_dir="$(cd "$(dirname "$0")" && pwd)"
sys_dir="${crate_dir}/src/sys"

for tool in bindgen rustfmt sha256sum; do
    if ! command -v "${tool}" > /dev/null; then
        echo "missing tool: ${tool}" >&2
        exit 1
    fi
done
if [[ "$(bindgen --version)" != "bindgen ${bindgen_version}" ]]; then
    echo "bindgen ${bindgen_version} is needed, found: $(bindgen --version)" >&2
    exit 1
fi
if [[ ! -d "${mingw_include}" ]]; then
    echo "missing the mingw-w64 headers in ${mingw_include}" >&2
    exit 1
fi
if ! grep -q "\"${ffmpeg_version}\"" "${include_dir}/libavutil/ffversion.h" 2> /dev/null; then
    echo "${include_dir} does not hold the headers of FFmpeg ${ffmpeg_version}" >&2
    exit 1
fi

# One digest for the whole header tree, in path order, so the same
# headers always give the same line at the top of the output.
headers_sha256="$(
    cd "${include_dir}"
    find libavcodec libavutil libswresample -name '*.h' -print0 \
        | sort -z \
        | xargs -0 sha256sum \
        | sha256sum \
        | cut -d' ' -f1
)"

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/zyr-codec-bindings.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT

cat > "${work_dir}/ffmpeg.h" << 'EOF'
#include <libavcodec/avcodec.h>
#include <libavutil/avutil.h>
#include <libavutil/channel_layout.h>
#include <libavutil/dict.h>
#include <libavutil/error.h>
#include <libavutil/ffversion.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/log.h>
#include <libswresample/swresample.h>
EOF
cat > "${work_dir}/d3d11va.h" << 'EOF'
#include <libavutil/hwcontext_d3d11va.h>
EOF

either() {
    local IFS='|'
    echo "$*"
}

common_options=(
    --rust-target 1.85
    --rust-edition 2024
    --use-core
    --no-doc-comments
    --default-enum-style newtype
    --generate-cstr
)

clang_options() {
    local target="$1"
    echo "--target=${target}"
    echo "-I${include_dir}"
    if [[ "${target}" == *-windows-* ]]; then
        echo "-isystem"
        echo "${mingw_include}"
    fi
}

# The function table of one library: a struct holding a pointer to each
# function, filled from the library at load time, then the list of the
# names it resolves, for the loader to name one that is missing.
table() {
    local target="$1" name="$2" list="$3"
    shift 3
    local functions=("$@")
    local clang
    mapfile -t clang < <(clang_options "${target}")
    bindgen "${work_dir}/ffmpeg.h" "${common_options[@]}" \
        --dynamic-loading "${name}" \
        --dynamic-link-require-all \
        --wrap-unsafe-ops \
        --allowlist-function "$(either "${functions[@]}")" \
        --blocklist-type '.*' \
        -- "${clang[@]}"
    printf 'pub const %s: &[&str] = &[\n' "${list}"
    printf '    "%s",\n' "${functions[@]}"
    printf '];\n'
}

generate() {
    local target="$1"
    local clang
    mapfile -t clang < <(clang_options "${target}")
    {
        bindgen "${work_dir}/ffmpeg.h" "${common_options[@]}" \
            --allowlist-type "$(either "${types[@]}")" \
            --allowlist-var "$(either "${constants[@]}")" \
            -- "${clang[@]}"
        table "${target}" Avutil AVUTIL_FUNCTIONS "${avutil_functions[@]}"
        table "${target}" Swresample SWRESAMPLE_FUNCTIONS "${swresample_functions[@]}"
        table "${target}" Avcodec AVCODEC_FUNCTIONS "${avcodec_functions[@]}"
    } | grep -v '^/\* automatically generated by rust-bindgen'
}

header() {
    local what="$1" targets_line="$2"
    cat << EOF
// FFmpeg ${ffmpeg_version}: ${what}.
//
// Generated by crates/zyr-codec/generate-bindings.sh with bindgen
// ${bindgen_version}; run it again rather than editing this file.
// ${targets_line}
// Headers: SHA-256 ${headers_sha256}
// (every header of libavcodec, libavutil and libswresample, in path order).

EOF
}

for target in "${targets[@]}"; do
    echo "Bindings for ${target}"
    generate "${target}" > "${work_dir}/${target}.rs"
done
if ! cmp -s "${work_dir}/${targets[0]}.rs" "${work_dir}/${targets[1]}.rs"; then
    echo "the bindings differ between ${targets[0]} and ${targets[1]}:" >&2
    diff -u "${work_dir}/${targets[0]}.rs" "${work_dir}/${targets[1]}.rs" >&2 || true
    exit 1
fi

{
    header "types, constants and function tables" \
        "Generated for ${targets[0]} and ${targets[1]}, identical for both."
    cat "${work_dir}/${targets[0]}.rs"
} > "${work_dir}/bindings.rs"

echo "Direct3D 11 structures for x86_64-pc-windows-gnu"
mapfile -t windows_clang < <(clang_options x86_64-pc-windows-gnu)
raw_lines=()
for line in "${d3d11va_names[@]}"; do
    raw_lines+=(--raw-line "${line}")
done
{
    header "the Direct3D 11 structures of hwcontext_d3d11va.h" \
        "Generated for x86_64-pc-windows-gnu against the mingw-w64 d3d11.h."
    bindgen "${work_dir}/d3d11va.h" "${common_options[@]}" \
        --allowlist-type "$(either "${d3d11va_types[@]}")" \
        --no-recursive-allowlist \
        "${raw_lines[@]}" \
        -- "${windows_clang[@]}" \
        | grep -v '^/\* automatically generated by rust-bindgen'
} > "${work_dir}/d3d11va.rs"

# Formatted aside, then renamed into place whole: nothing that reads the
# crate meanwhile, a build or a formatter, ever sees half a file.
mkdir -p "${sys_dir}"
for name in bindings.rs d3d11va.rs; do
    rustfmt --edition 2024 "${work_dir}/${name}"
    staged="$(mktemp "${sys_dir}/.${name}.XXXXXX")"
    cat "${work_dir}/${name}" > "${staged}"
    chmod 644 "${staged}"
    mv -f "${staged}" "${sys_dir}/${name}"
done
echo "Written: ${sys_dir}/bindings.rs, ${sys_dir}/d3d11va.rs"
