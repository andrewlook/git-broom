#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   ./asciinema-export.sh <asciinema-url> <output-dir> [name]

usage() {
    cat << 'EOF'
Usage:
  asciinema-export.sh <asciinema-url> <output-dir> [name]

Installs missing dependencies (with confirmation):
  - curl
  - agg
  - svg-term-cli
  - ffmpeg
EOF
}

RED='\033[0;31m'
NC='\033[0m' # No Color

error_red() {
    echo -e "${RED}error:${NC} $*" >&2
}

die() {
    echo "error: $*" >&2
    exit 1
}

confirm() {
    read -r -p "$1 [y/N]: " response
    [[ "$response" =~ ^[Yy]$ ]]
}

detect_platform() {
    case "$(uname -s)" in
        Darwin) echo "macos" ;;
        Linux) echo "linux" ;;
        *) echo "unknown" ;;
    esac
}

require_dev_tools() {
    local platform="$1"

    if ! command -v npm > /dev/null 2>&1; then
        error_red "npm is required for svg-term but was not found."
        echo "Install Node.js (includes npm): https://nodejs.org/"
        exit 1
    fi

    if [[ "$platform" == "linux" ]]; then
        if ! command -v cargo > /dev/null 2>&1; then
            error_red "cargo is required to install agg on Linux but was not found."
            echo "Install Rust (includes cargo): https://rustup.rs/"
            exit 1
        fi
    fi
}

#
# Verify cargo/npm installed first
#
platform="$(detect_platform)"
require_dev_tools "$platform"

install_cmd() {
    local cmd="$1"
    local platform="$2"

    echo "Attempting to install: $cmd"

    if [[ "$platform" == "macos" ]]; then
        if ! command -v brew > /dev/null 2>&1; then
            die "Homebrew not found. Please install it from https://brew.sh"
        fi

        case "$cmd" in
            curl) brew install curl ;;
            ffmpeg) brew install ffmpeg ;;
            agg) cargo install --git https://github.com/asciinema/agg ;;
            svg-term) npm install -g svg-term-cli ;;
            *) die "No install rule for $cmd" ;;
        esac

    elif [[ "$platform" == "linux" ]]; then
        if command -v apt-get > /dev/null 2>&1; then
            case "$cmd" in
                curl) sudo apt-get update && sudo apt-get install -y curl ;;
                ffmpeg) sudo apt-get install -y ffmpeg ;;
                agg) cargo install --git https://github.com/asciinema/agg ;;
                svg-term) npm install -g svg-term-cli ;;
                *) die "No install rule for $cmd" ;;
            esac
        else
            die "Unsupported Linux package manager. Install $cmd manually."
        fi
    else
        die "Unsupported platform. Please install $cmd manually."
    fi
}

ensure_cmd() {
    local cmd="$1"
    local platform="$2"

    if command -v "$cmd" > /dev/null 2>&1; then
        return 0
    fi

    echo "Missing dependency: $cmd"

    if confirm "Would you like to install $cmd?"; then
        install_cmd "$cmd" "$platform"
    else
        die "$cmd is required"
    fi

    command -v "$cmd" > /dev/null 2>&1 || die "Failed to install $cmd"
}

extract_id() {
    local url="$1"
    url="${url%%\?*}"
    url="${url%%\#*}"

    if [[ "$url" =~ /a/([^/.]+) ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
    else
        return 1
    fi
}

download_cast() {
    local id="$1"
    local dest="$2"

    local urls=(
        "https://asciinema.org/a/${id}.cast"
        "https://asciinema.org/a/${id}.json"
    )

    for u in "${urls[@]}"; do
        if curl -fsSL "$u" -o "$dest.tmp"; then
            if head -n1 "$dest.tmp" | grep -q '^{'; then
                mv "$dest.tmp" "$dest"
                return 0
            fi
        fi
    done

    return 1
}

main() {
    [[ $# -ge 2 && $# -le 3 ]] || {
        usage
        exit 1
    }

    local url="$1"
    local outdir="$2"
    local name="${3:-}"

    local platform
    platform="$(detect_platform)"

    # Ensure dependencies
    ensure_cmd curl "$platform"
    ensure_cmd ffmpeg "$platform"
    ensure_cmd agg "$platform"
    ensure_cmd svg-term "$platform"

    local id
    id="$(extract_id "$url")" || die "Invalid asciinema URL"

    [[ -n "$name" ]] || name="$id"

    mkdir -p "$outdir"

    local cast="${outdir}/${name}.cast"
    local gif="${outdir}/${name}.gif"
    local svg="${outdir}/${name}.svg"
    local mp4="${outdir}/${name}.mp4"

    echo "Downloading cast..."
    download_cast "$id" "$cast" || die "Failed to download cast"

    echo "Generating GIF..."
    agg "$cast" "$gif"

    echo "Generating SVG..."
    svg-term --in "$cast" --out "$svg" --window

    echo "Generating MP4..."
    ffmpeg -y -i "$gif" -movflags +faststart -pix_fmt yuv420p \
        -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" \
        "$mp4" > /dev/null 2>&1

    cat << EOF

Done!

Files:
  $cast
  $gif
  $svg
  $mp4

README:

  [![demo](./$(basename "$outdir")/${name}.svg)](${url})

EOF
}

main "$@"
