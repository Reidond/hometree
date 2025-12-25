#!/usr/bin/env bash
#
# install.sh - Install hometree from GitHub releases
#
# Usage:
#   ./install.sh                      Install latest version
#   ./install.sh --version v0.5.0     Install specific version
#   ./install.sh --prefix ~/.local    Install to custom prefix (default: ~/.local)
#   ./install.sh --help               Show this help
#
# Environment:
#   HOMETREE_PREFIX   Override install prefix
#   NO_COLOR          Disable colored output
#

set -euo pipefail

readonly REPO="Reidond/hometree"
readonly BINARY_NAME="hometree"
readonly DEFAULT_PREFIX="${HOMETREE_PREFIX:-$HOME/.local}"

setup_colors() {
    if [[ -t 1 ]] && [[ -z "${NO_COLOR:-}" ]]; then
        RED='\033[0;31m'
        GREEN='\033[0;32m'
        YELLOW='\033[0;33m'
        BLUE='\033[0;34m'
        BOLD='\033[1m'
        RESET='\033[0m'
    else
        RED='' GREEN='' YELLOW='' BLUE='' BOLD='' RESET=''
    fi
}

log_info() { printf "${BLUE}[INFO]${RESET} %s\n" "$*"; }
log_success() { printf "${GREEN}[OK]${RESET} %s\n" "$*"; }
log_warn() { printf "${YELLOW}[WARN]${RESET} %s\n" "$*" >&2; }
log_error() { printf "${RED}[ERROR]${RESET} %s\n" "$*" >&2; }
log_fatal() { log_error "$*"; exit 1; }

command_exists() { command -v "$1" &>/dev/null; }

detect_target() {
    local os arch target

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="unknown-linux" ;;
        Darwin) os="apple-darwin" ;;
        *)      log_fatal "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             log_fatal "Unsupported architecture: $arch" ;;
    esac

    if [[ "$os" == "unknown-linux" ]]; then
        if ldd --version 2>&1 | grep -q musl; then
            target="${arch}-unknown-linux-musl"
        else
            target="${arch}-unknown-linux-gnu"
        fi
    else
        target="${arch}-${os}"
    fi

    echo "$target"
}

get_latest_version() {
    local url="https://api.github.com/repos/${REPO}/releases/latest"
    
    if command_exists curl; then
        curl -fsSL "$url" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
    elif command_exists wget; then
        wget -qO- "$url" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
    else
        log_fatal "curl or wget required"
    fi
}

download_file() {
    local url="$1"
    local dest="$2"

    log_info "Downloading: $url"

    if command_exists curl; then
        curl -fsSL "$url" -o "$dest"
    elif command_exists wget; then
        wget -q "$url" -O "$dest"
    else
        log_fatal "curl or wget required"
    fi
}

verify_checksum() {
    local file="$1"
    local checksums_url="$2"
    local expected

    if ! command_exists sha256sum; then
        log_warn "sha256sum not found, skipping checksum verification"
        return 0
    fi

    local checksums_file
    checksums_file="$(mktemp)"
    download_file "$checksums_url" "$checksums_file"

    local filename
    filename="$(basename "$file")"
    expected="$(grep "$filename" "$checksums_file" | awk '{print $1}')"
    rm -f "$checksums_file"

    if [[ -z "$expected" ]]; then
        log_warn "Checksum not found for $filename, skipping verification"
        return 0
    fi

    local actual
    actual="$(sha256sum "$file" | awk '{print $1}')"

    if [[ "$actual" != "$expected" ]]; then
        log_fatal "Checksum mismatch! Expected: $expected, Got: $actual"
    fi

    log_success "Checksum verified"
}

install_binary() {
    local version="$1"
    local prefix="$2"
    local target="$3"
    local bin_dir="${prefix}/bin"

    local archive_name="${BINARY_NAME}-${target}.tar.gz"
    local download_url="https://github.com/${REPO}/releases/download/${version}/${archive_name}"
    local checksums_url="https://github.com/${REPO}/releases/download/${version}/checksums-sha256.txt"

    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' EXIT

    local archive_path="${tmp_dir}/${archive_name}"
    download_file "$download_url" "$archive_path"
    verify_checksum "$archive_path" "$checksums_url"

    log_info "Extracting..."
    tar -xzf "$archive_path" -C "$tmp_dir"

    mkdir -p "$bin_dir"
    install -m 755 "${tmp_dir}/${BINARY_NAME}" "${bin_dir}/${BINARY_NAME}"

    log_success "Installed: ${bin_dir}/${BINARY_NAME}"
}

update_path_instructions() {
    local bin_dir="$1"

    if [[ ":$PATH:" == *":${bin_dir}:"* ]]; then
        return 0
    fi

    log_warn "${bin_dir} is not in your PATH."
    printf "\n${BOLD}Add to your shell config:${RESET}\n"

    case "${SHELL:-}" in
        */fish)
            printf "  ${BLUE}fish_add_path %s${RESET}\n" "$bin_dir"
            ;;
        *)
            printf "  ${BLUE}export PATH=\"%s:\$PATH\"${RESET}\n" "$bin_dir"
            ;;
    esac
    printf "\n"
}

show_help() {
    cat <<EOF
${BOLD}hometree installer${RESET}

Install hometree from GitHub releases.

${BOLD}USAGE:${RESET}
    ./install.sh [OPTIONS]

${BOLD}OPTIONS:${RESET}
    --version, -v VERSION   Install specific version (default: latest)
    --prefix DIR            Install to DIR/bin (default: ~/.local)
    --help, -h              Show this help message

${BOLD}EXAMPLES:${RESET}
    ./install.sh                        # Install latest
    ./install.sh --version v0.5.0       # Install specific version
    ./install.sh --prefix /opt/local    # Install to /opt/local/bin

${BOLD}ENVIRONMENT:${RESET}
    HOMETREE_PREFIX   Override default prefix
    NO_COLOR          Disable colored output

EOF
}

main() {
    setup_colors

    local version=""
    local prefix="$DEFAULT_PREFIX"

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version|-v)
                [[ -z "${2:-}" ]] && log_fatal "--version requires an argument"
                version="$2"
                shift 2
                ;;
            --prefix)
                [[ -z "${2:-}" ]] && log_fatal "--prefix requires an argument"
                prefix="$2"
                shift 2
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_fatal "Unknown option: $1. Use --help for usage."
                ;;
        esac
    done

    local target
    target="$(detect_target)"
    log_info "Detected target: $target"

    if [[ -z "$version" ]]; then
        log_info "Fetching latest version..."
        version="$(get_latest_version)"
        if [[ -z "$version" ]]; then
            log_fatal "Could not determine latest version"
        fi
    fi

    printf "\n${BOLD}hometree installer${RESET}\n"
    printf "Version: ${BLUE}%s${RESET}\n" "$version"
    printf "Target:  ${BLUE}%s${RESET}\n" "$target"
    printf "Prefix:  ${BLUE}%s${RESET}\n\n" "$prefix"

    install_binary "$version" "$prefix" "$target"

    update_path_instructions "${prefix}/bin"

    printf "${GREEN}${BOLD}Installation complete!${RESET}\n"
    printf "Run ${BLUE}hometree --help${RESET} to get started.\n"
}

main "$@"
