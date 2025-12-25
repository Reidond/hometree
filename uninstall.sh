#!/usr/bin/env bash
#
# uninstall.sh - Rootless uninstaller for hometree
#
# Usage:
#   ./uninstall.sh              Uninstall from default locations
#   ./uninstall.sh --prefix DIR Uninstall from custom prefix
#   ./uninstall.sh --purge      Also remove config and data directories
#   ./uninstall.sh --dry-run    Show what would be removed without removing
#   ./uninstall.sh --help       Show this help
#
# Environment:
#   HOMETREE_PREFIX   Override install prefix (default: ~/.local)
#   CARGO_HOME        Override cargo home (default: ~/.cargo)
#   NO_COLOR          Disable colored output
#

set -euo pipefail

# --- Constants ---
readonly BINARY_NAME="hometree"

# --- Color support ---
setup_colors() {
    if [[ -t 1 ]] && [[ -z "${NO_COLOR:-}" ]]; then
        RED='\033[0;31m'
        GREEN='\033[0;32m'
        YELLOW='\033[0;33m'
        BLUE='\033[0;34m'
        BOLD='\033[1m'
        RESET='\033[0m'
    else
        RED=''
        GREEN=''
        YELLOW=''
        BLUE=''
        BOLD=''
        RESET=''
    fi
}

# --- Logging ---
log_info() { printf "${BLUE}[INFO]${RESET} %s\n" "$*"; }
log_success() { printf "${GREEN}[OK]${RESET} %s\n" "$*"; }
log_warn() { printf "${YELLOW}[WARN]${RESET} %s\n" "$*" >&2; }
log_error() { printf "${RED}[ERROR]${RESET} %s\n" "$*" >&2; }
log_dry() { printf "${YELLOW}[DRY-RUN]${RESET} Would remove: %s\n" "$*"; }

# --- Utilities ---
command_exists() { command -v "$1" &>/dev/null; }

confirm() {
    local prompt="$1"
    local response
    printf "${YELLOW}%s [y/N]:${RESET} " "$prompt"
    read -r response
    [[ "$response" =~ ^[Yy]$ ]]
}

safe_remove() {
    local path="$1"
    local dry_run="${2:-false}"

    if [[ ! -e "$path" ]]; then
        return 0
    fi

    if [[ "$dry_run" == true ]]; then
        log_dry "$path"
        return 0
    fi

    if [[ -d "$path" ]]; then
        rm -rf "$path"
    else
        rm -f "$path"
    fi
    log_success "Removed: $path"
}

# --- Uninstall functions ---
stop_systemd_service() {
    local dry_run="${1:-false}"

    if [[ "$(uname -s)" != "Linux" ]]; then
        return 0
    fi

    if ! command_exists systemctl; then
        return 0
    fi

    if systemctl --user is-active --quiet hometree.service 2>/dev/null; then
        if [[ "$dry_run" == true ]]; then
            log_dry "systemctl --user stop hometree.service"
        else
            log_info "Stopping hometree service..."
            systemctl --user stop hometree.service 2>/dev/null || true
            systemctl --user disable hometree.service 2>/dev/null || true
        fi
    fi
}

uninstall_systemd_service() {
    local dry_run="${1:-false}"
    local config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
    local unit_path="${config_dir}/systemd/user/hometree.service"

    if [[ -f "$unit_path" ]]; then
        safe_remove "$unit_path" "$dry_run"
        if [[ "$dry_run" != true ]] && command_exists systemctl; then
            systemctl --user daemon-reload 2>/dev/null || true
        fi
    fi
}

find_binary_locations() {
    local prefix="${1:-}"
    local locations=()

    local search_paths=(
        "${prefix:+${prefix}/bin/${BINARY_NAME}}"
        "${HOMETREE_PREFIX:-$HOME/.local}/bin/${BINARY_NAME}"
        "${CARGO_HOME:-$HOME/.cargo}/bin/${BINARY_NAME}"
        "$HOME/.local/bin/${BINARY_NAME}"
        "$HOME/.cargo/bin/${BINARY_NAME}"
    )

    for path in "${search_paths[@]}"; do
        [[ -z "$path" ]] && continue
        if [[ -f "$path" ]]; then
            local exists=false
            for loc in "${locations[@]:-}"; do
                [[ "$loc" == "$path" ]] && exists=true && break
            done
            [[ "$exists" == false ]] && locations+=("$path")
        fi
    done

    local path_binary
    path_binary="$(command -v "$BINARY_NAME" 2>/dev/null || true)"
    if [[ -n "$path_binary" && -f "$path_binary" ]]; then
        local exists=false
        for loc in "${locations[@]:-}"; do
            [[ "$loc" == "$path_binary" ]] && exists=true && break
        done
        [[ "$exists" == false ]] && locations+=("$path_binary")
    fi

    printf '%s\n' "${locations[@]:-}"
}

get_data_directories() {
    local config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/hometree"
    local data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/hometree"
    local state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/hometree"
    local cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/hometree"
    local runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/hometree"

    local dirs=()
    [[ -d "$config_dir" ]] && dirs+=("$config_dir")
    [[ -d "$data_dir" ]] && dirs+=("$data_dir")
    [[ -d "$state_dir" ]] && dirs+=("$state_dir")
    [[ -d "$cache_dir" ]] && dirs+=("$cache_dir")
    [[ -d "$runtime_dir" ]] && dirs+=("$runtime_dir")

    printf '%s\n' "${dirs[@]:-}"
}

# --- Help ---
show_help() {
    cat <<EOF
${BOLD}hometree uninstaller${RESET}

${BOLD}USAGE:${RESET}
    ./uninstall.sh [OPTIONS]

${BOLD}OPTIONS:${RESET}
    --prefix DIR    Uninstall from DIR/bin
    --purge         Also remove config and data directories
    --dry-run       Show what would be removed without removing
    --force, -f     Skip confirmation prompts
    --help, -h      Show this help message

${BOLD}EXAMPLES:${RESET}
    ./uninstall.sh              # Uninstall binary and systemd service
    ./uninstall.sh --dry-run    # Preview what would be removed
    ./uninstall.sh --purge      # Also remove all hometree data
    ./uninstall.sh -f --purge   # Remove everything without prompts

${BOLD}LOCATIONS CHECKED:${RESET}
    Binary:   ~/.local/bin/hometree, ~/.cargo/bin/hometree
    Service:  ~/.config/systemd/user/hometree.service
    Config:   ~/.config/hometree/
    Data:     ~/.local/share/hometree/
    State:    ~/.local/state/hometree/
    Cache:    ~/.cache/hometree/

${BOLD}ENVIRONMENT:${RESET}
    HOMETREE_PREFIX   Override default prefix
    CARGO_HOME        Override cargo home directory
    NO_COLOR          Disable colored output

EOF
}

# --- Main ---
main() {
    setup_colors

    local prefix=""
    local purge=false
    local dry_run=false
    local force=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --prefix)
                [[ -z "${2:-}" ]] && { log_error "--prefix requires an argument"; exit 1; }
                prefix="$2"
                shift 2
                ;;
            --purge)
                purge=true
                shift
                ;;
            --dry-run)
                dry_run=true
                shift
                ;;
            --force|-f)
                force=true
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1. Use --help for usage."
                exit 1
                ;;
        esac
    done

    printf "${BOLD}hometree uninstaller${RESET}\n\n"

    local binaries
    binaries="$(find_binary_locations "$prefix")"

    local data_dirs=""
    if [[ "$purge" == true ]]; then
        data_dirs="$(get_data_directories)"
    fi

    local has_service=false
    local service_path="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/hometree.service"
    [[ -f "$service_path" ]] && has_service=true

    if [[ -z "$binaries" && "$has_service" == false && -z "$data_dirs" ]]; then
        log_info "hometree is not installed or already uninstalled."
        exit 0
    fi

    printf "${BOLD}The following will be removed:${RESET}\n\n"

    if [[ -n "$binaries" ]]; then
        printf "  ${BLUE}Binaries:${RESET}\n"
        while IFS= read -r bin; do
            printf "    - %s\n" "$bin"
        done <<< "$binaries"
    fi

    if [[ "$has_service" == true ]]; then
        printf "  ${BLUE}Systemd service:${RESET}\n"
        printf "    - %s\n" "$service_path"
    fi

    if [[ -n "$data_dirs" ]]; then
        printf "  ${BLUE}Data directories:${RESET}\n"
        while IFS= read -r dir; do
            printf "    - %s\n" "$dir"
        done <<< "$data_dirs"
    fi

    printf "\n"

    if [[ "$dry_run" == true ]]; then
        log_info "Dry run mode - no changes will be made."
        printf "\n"
    elif [[ "$force" != true ]]; then
        if ! confirm "Proceed with uninstall?"; then
            log_info "Aborted."
            exit 0
        fi
        printf "\n"
    fi

    stop_systemd_service "$dry_run"

    if [[ "$has_service" == true ]]; then
        uninstall_systemd_service "$dry_run"
    fi

    if [[ -n "$binaries" ]]; then
        while IFS= read -r bin; do
            safe_remove "$bin" "$dry_run"
        done <<< "$binaries"
    fi

    if [[ -n "$data_dirs" ]]; then
        while IFS= read -r dir; do
            safe_remove "$dir" "$dry_run"
        done <<< "$data_dirs"
    fi

    printf "\n"
    if [[ "$dry_run" == true ]]; then
        log_info "Dry run complete. No changes were made."
    else
        log_success "hometree has been uninstalled."
        if [[ "$purge" != true ]]; then
            log_info "Config/data directories preserved. Use --purge to remove them."
        fi
    fi
}

main "$@"
