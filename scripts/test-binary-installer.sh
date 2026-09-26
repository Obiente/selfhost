#!/bin/sh
set -eu
fixture=$(cd "$1" && pwd -P)
installer=$(cd "$(dirname "$0")" && pwd -P)/install.sh
version=$(node -p 'require("./packages/selfhost/package.json").version')
test_root=$(mktemp -d)
trap 'rm -rf "$test_root"' EXIT HUP INT TERM
mkdir "$test_root/tools"
cat > "$test_root/tools/curl" <<'SH'
#!/bin/sh
set -eu
url= output=
while [ "$#" -gt 0 ]; do
    case "$1" in
        "https://github.com/Obiente/selfhost/releases/download/v$TEST_VERSION/"*) url=$1; shift ;;
        -o) output=$2; shift 2 ;;
        *) shift ;;
    esac
done
[ -n "$url" ] && [ -n "$output" ]
name=${url##*/}
if [ "${CORRUPT_CHECKSUM:-false}" = true ] && [ "$name" = BINARY-SHA256SUMS ]; then
    sed 's/^[a-f0-9]\{64\}/0000000000000000000000000000000000000000000000000000000000000000/' "$FIXTURE/$name" > "$output"
else
    cp "$FIXTURE/$name" "$output"
fi
SH
chmod +x "$test_root/tools/curl"
export FIXTURE=$fixture
export TEST_VERSION=$version
export PATH="$test_root/tools:$PATH"
bin_dir=$test_root/bin\ with\ spaces
sh "$installer" --version "$version" --bin-dir "$bin_dir"
cp "$bin_dir/selfhost" "$test_root/original"
if sh "$installer" --version "$version" --bin-dir "$bin_dir"; then echo 'Existing binary was overwritten without approval.' >&2; exit 1; fi
export CORRUPT_CHECKSUM=true
if sh "$installer" --version "$version" --bin-dir "$bin_dir" --force; then echo 'Corrupt checksum was accepted.' >&2; exit 1; fi
cmp "$bin_dir/selfhost" "$test_root/original"
export CORRUPT_CHECKSUM=false
sh "$installer" --version "$version" --bin-dir "$bin_dir" --force
cmp "$bin_dir"/selfhost.backup.* "$test_root/original"
printf '%s\n' 'Installer passed: fresh install, explicit replacement, corrupt download rejection and backup.'
