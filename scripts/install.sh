#!/bin/sh
# Install a checksum-verified Selfhost release without a package manager.
set -eu

version=latest
bin_dir=${HOME:?HOME must be set}/.local/bin
force=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) version=${2:?Missing version}; shift 2 ;;
        --bin-dir) bin_dir=${2:?Missing installation directory}; shift 2 ;;
        --force) force=true; shift ;;
        --help) printf '%s\n' 'Usage: sh install.sh [--version VERSION] [--bin-dir DIRECTORY] [--force]'; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
    esac
done
case "$version" in
    latest) release=latest/download ;;
    *) version=${version#v}
       printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo 'Expected a stable version such as 0.1.1.' >&2; exit 1; }
       release=download/v$version ;;
esac
case "$(uname -s)" in
    Linux) platform=linux ;;
    Darwin) platform=darwin ;;
    *) echo 'Use install.ps1 on Windows. Only Linux and macOS are supported by this installer.' >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) echo 'Supported architectures: x64 and ARM64.' >&2; exit 1 ;;
esac
command -v curl >/dev/null || { echo 'curl is required.' >&2; exit 1; }
asset=selfhost-$platform-$arch
base=https://github.com/Obiente/selfhost/releases/$release
mkdir -p "$bin_dir"
bin_dir=$(cd "$bin_dir" && pwd -P)
target=$bin_dir/selfhost
if [ -e "$target" ] || [ -L "$target" ]; then
    [ "$force" = true ] || { echo 'Selfhost already exists here. Use --force to update it with a backup.' >&2; exit 1; }
    [ -f "$target" ] && [ ! -L "$target" ] || { echo 'Refusing to replace a symlink or non-file.' >&2; exit 1; }
fi
stage=$(mktemp -d "$bin_dir/.selfhost-install.XXXXXXXX")
# mktemp created this exact directory under the resolved installation directory.
trap 'rm -f "$stage/binary" "$stage/checksums"; rmdir "$stage"' EXIT HUP INT TERM
curl --proto '=https' --tlsv1.2 -fsSL "$base/BINARY-SHA256SUMS" -o "$stage/checksums"
curl --proto '=https' --tlsv1.2 -fsSL "$base/$asset" -o "$stage/binary"
expected=$(awk -v name="$asset" '$2 == name { print $1 }' "$stage/checksums")
printf '%s\n' "$expected" | grep -Eq '^[a-f0-9]{64}$' || { echo 'Missing or ambiguous release checksum.' >&2; exit 1; }
if command -v sha256sum >/dev/null; then
    actual=$(sha256sum "$stage/binary" | awk '{print $1}')
elif command -v shasum >/dev/null; then
    actual=$(shasum -a 256 "$stage/binary" | awk '{print $1}')
else
    echo 'sha256sum or shasum is required.' >&2; exit 1
fi
[ "$actual" = "$expected" ] || { echo 'Checksum mismatch. Existing installation was not changed.' >&2; exit 1; }
chmod 755 "$stage/binary"
installed_version=$("$stage/binary" --version)
if [ "$version" != latest ]; then
    [ "$installed_version" = "selfhost $version" ] || { echo 'Downloaded binary reports a different version.' >&2; exit 1; }
fi
if [ -e "$target" ]; then
    backup=$target.backup.$(date +%s).$$
    # A hard link retains the current binary before the atomic replacement.
    ln "$target" "$backup"
    printf 'Previous binary kept at %s\n' "$backup"
fi
mv -f "$stage/binary" "$target"
printf 'Installed %s at %s\n' "$installed_version" "$target"
case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) printf 'Add this directory to PATH in your shell configuration: %s\n' "$bin_dir" ;;
esac
printf '%s\n' 'Run selfhost --help or selfhost serve. No services were started.'
