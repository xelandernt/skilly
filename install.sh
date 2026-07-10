#!/bin/sh
# shellcheck shell=sh
set -eu

# skilly installer for macOS and Linux.
# Usage: curl -fsSL --proto '=https' --tlsv1.2 \
#   https://raw.githubusercontent.com/xelandernt/skilly/main/install.sh | sh

REPO="xelandernt/skilly"
BINARY="skilly"

DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

VERSION="${SKILLY_VERSION:-latest}"
INSTALL_DIR="${SKILLY_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
NO_MODIFY_PATH="${SKILLY_NO_MODIFY_PATH:-0}"
DRY_RUN="${SKILLY_DRY_RUN:-0}"
VERBOSE="${SKILLY_VERBOSE:-0}"

DOWNLOADER=""
TMPDIR_INSTALL=""

cleanup() {
    if [ -n "$TMPDIR_INSTALL" ] && [ -d "$TMPDIR_INSTALL" ]; then
        rm -rf "$TMPDIR_INSTALL"
    fi
}
trap cleanup EXIT INT TERM

info() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; }
die() {
    err "$*"
    exit 1
}
debug() {
    if [ "$VERBOSE" = "1" ]; then
        printf 'debug: %s\n' "$*" >&2
    fi
}

usage() {
    cat <<EOF
skilly installer (macOS + Linux)

Usage:
  curl -fsSL --proto '=https' --tlsv1.2 \\
    https://raw.githubusercontent.com/${REPO}/main/install.sh | sh
  curl -fsSL --proto '=https' --tlsv1.2 \\
    https://raw.githubusercontent.com/${REPO}/main/install.sh | sh -s -- [options]

Options:
  --version <version>   Version to install (default: latest, e.g. 0.0.32)
  --to <dir>            Install directory (default: ~/.local/bin)
  --no-modify-path      Do not add the install directory to your shell PATH
  --dry-run             Print what would happen without installing
  --verbose             Print debug output
  --help, -h            Show this help

Environment variables:
  SKILLY_VERSION           Same as --version
  SKILLY_INSTALL_DIR       Same as --to
  SKILLY_NO_MODIFY_PATH=1  Same as --no-modify-path
  SKILLY_DRY_RUN=1         Same as --dry-run
  SKILLY_VERBOSE=1         Same as --verbose
  SKILLY_GITHUB_TOKEN      GitHub token for higher API rate limits (latest lookup)
EOF
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                [ $# -ge 2 ] || die "missing value for --version"
                VERSION="$2"
                shift 2
                ;;
            --version=*)
                VERSION="${1#*=}"
                shift
                ;;
            --to)
                [ $# -ge 2 ] || die "missing value for --to"
                INSTALL_DIR="$2"
                shift 2
                ;;
            --to=*)
                INSTALL_DIR="${1#*=}"
                shift
                ;;
            --no-modify-path)
                NO_MODIFY_PATH=1
                shift
                ;;
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --verbose)
                VERBOSE=1
                shift
                ;;
            --help | -h)
                usage
                exit 0
                ;;
            *)
                die "unknown option: $1 (use --help)"
                ;;
        esac
    done
}

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        die "required command not found: $1"
    fi
}

detect_downloader() {
    if command -v curl >/dev/null 2>&1; then
        DOWNLOADER="curl"
    elif command -v wget >/dev/null 2>&1; then
        DOWNLOADER="wget"
    else
        die "need curl or wget to download files"
    fi
    debug "using downloader: $DOWNLOADER"
}

# download_to <url> <output>
download_to() {
    _url="$1"
    _out="$2"
    if [ "$DOWNLOADER" = "curl" ]; then
        curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$_out" "$_url"
    else
        wget -q --https-only --secure-protocol=TLSv1_2 -O "$_out" "$_url"
    fi
}

# download_stdout <url>
download_stdout() {
    _url="$1"
    if [ "$DOWNLOADER" = "curl" ]; then
        _auth=""
        if [ -n "${SKILLY_GITHUB_TOKEN:-}" ]; then
            curl -fsSL --proto '=https' --tlsv1.2 --retry 3 \
                -H "Authorization: Bearer ${SKILLY_GITHUB_TOKEN}" "$_url"
        else
            curl -fsSL --proto '=https' --tlsv1.2 --retry 3 "$_url"
        fi
    else
        if [ -n "${SKILLY_GITHUB_TOKEN:-}" ]; then
            wget -q --https-only --secure-protocol=TLSv1_2 \
                --header="Authorization: Bearer ${SKILLY_GITHUB_TOKEN}" -O - "$_url"
        else
            wget -q --https-only --secure-protocol=TLSv1_2 -O - "$_url"
        fi
    fi
}

detect_target() {
    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Darwin) _os_name="apple-darwin" ;;
        Linux) _os_name="unknown-linux-gnu" ;;
        *) die "unsupported operating system: ${_os} (skilly supports macOS and Linux via this script)" ;;
    esac

    case "$_arch" in
        x86_64 | amd64) _arch_name="x86_64" ;;
        arm64 | aarch64) _arch_name="aarch64" ;;
        *) die "unsupported architecture: ${_arch}" ;;
    esac

    TARGET="${_arch_name}-${_os_name}"

    case "$TARGET" in
        aarch64-apple-darwin | x86_64-apple-darwin | x86_64-unknown-linux-gnu) ;;
        *)
            die "no prebuilt skilly binary for ${TARGET}. Supported targets: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu. Install via 'uvx skilly', 'npx @xelandernt/skilly', or 'cargo install' instead."
            ;;
    esac
    debug "detected target: $TARGET"
}

resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        return
    fi
    debug "resolving latest release via GitHub API"
    _api="https://api.github.com/repos/${REPO}/releases/latest"
    _json="$(download_stdout "$_api")" || die "failed to query latest release"
    VERSION="$(printf '%s\n' "$_json" \
        | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' \
        | head -n1 \
        | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
    [ -n "$VERSION" ] || die "could not determine latest version from GitHub API"
    debug "latest version: $VERSION"
}

verify_checksum() {
    _archive="$1"
    _checksums="$2"
    _name="$3"
    _expected="$(grep " ${_name}\$" "$_checksums" | head -n1 | awk '{print $1}')"
    [ -n "$_expected" ] || die "no checksum found for ${_name}"

    if command -v sha256sum >/dev/null 2>&1; then
        _actual="$(sha256sum "$_archive" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        _actual="$(shasum -a 256 "$_archive" | awk '{print $1}')"
    else
        die "need sha256sum or shasum to verify the download"
    fi

    if [ "$_expected" != "$_actual" ]; then
        die "checksum mismatch for ${_name}: expected ${_expected}, got ${_actual}"
    fi
    debug "checksum verified: $_name"
}

path_contains() {
    case ":${PATH}:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

modify_path() {
    _dir="$1"
    if [ "$NO_MODIFY_PATH" = "1" ]; then
        return
    fi
    if path_contains "$_dir"; then
        debug "install dir already on PATH"
        return
    fi

    _line="export PATH=\"${_dir}:\$PATH\""
    _updated=0
    for _rc in "${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.profile"; do
        if [ -f "$_rc" ]; then
            if ! grep -Fqx "$_line" "$_rc" 2>/dev/null; then
                printf '\n# added by skilly installer\n%s\n' "$_line" >>"$_rc"
                info "Updated PATH in ${_rc}"
            fi
            _updated=1
        fi
    done
    if [ "$_updated" = "0" ]; then
        printf '# added by skilly installer\n%s\n' "$_line" >>"${HOME}/.profile"
        info "Created ${HOME}/.profile and added ${_dir} to PATH"
    fi
    info "Restart your shell or run: ${_line}"
}

main() {
    parse_args "$@"
    detect_downloader
    need_cmd tar
    detect_target
    resolve_version

    _base="https://github.com/${REPO}/releases/download/${VERSION}"
    _archive_name="${BINARY}-${VERSION}-${TARGET}.tar.gz"
    _archive_url="${_base}/${_archive_name}"
    _checksums_url="${_base}/${BINARY}-sha256sums.txt"

    info "Installing ${BINARY} ${VERSION} (${TARGET}) to ${INSTALL_DIR}"

    if [ "$DRY_RUN" = "1" ]; then
        info "[dry-run] would download: ${_archive_url}"
        info "[dry-run] would verify against: ${_checksums_url}"
        info "[dry-run] would install to: ${INSTALL_DIR}/${BINARY}"
        return
    fi

    TMPDIR_INSTALL="$(mktemp -d 2>/dev/null || mktemp -d -t skilly)"
    _archive="${TMPDIR_INSTALL}/${_archive_name}"
    _checksums="${TMPDIR_INSTALL}/${BINARY}-sha256sums.txt"

    debug "downloading ${_archive_url}"
    download_to "$_archive_url" "$_archive" \
        || die "failed to download ${_archive_url} (does version ${VERSION} exist?)"
    debug "downloading ${_checksums_url}"
    download_to "$_checksums_url" "$_checksums" \
        || die "failed to download checksums from ${_checksums_url}"

    verify_checksum "$_archive" "$_checksums" "$_archive_name"

    tar -xzf "$_archive" -C "$TMPDIR_INSTALL" || die "failed to extract ${_archive_name}"
    [ -f "${TMPDIR_INSTALL}/${BINARY}" ] || die "archive did not contain ${BINARY}"

    mkdir -p "$INSTALL_DIR" || die "failed to create ${INSTALL_DIR}"
    install -m 0755 "${TMPDIR_INSTALL}/${BINARY}" "${INSTALL_DIR}/${BINARY}" 2>/dev/null \
        || {
            cp "${TMPDIR_INSTALL}/${BINARY}" "${INSTALL_DIR}/${BINARY}" \
                && chmod 0755 "${INSTALL_DIR}/${BINARY}"
        } \
        || die "failed to install ${BINARY} to ${INSTALL_DIR}"

    info "Installed ${BINARY} to ${INSTALL_DIR}/${BINARY}"
    modify_path "$INSTALL_DIR"
    info "Run '${BINARY} --help' to get started."
}

main "$@"
