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
bin_dir="$test_root/bin with spaces and 'quote' and \$dollar"
test_home=$test_root/user
mkdir -p "$test_home" "$test_root/zsh" "$test_root/config/fish/conf.d"
printf '%s\n' '# Existing login configuration' > "$test_home/.bash_profile"
install() {
    env HOME="$test_home" XDG_DATA_HOME="$test_root/data" XDG_CONFIG_HOME="$test_root/config" ZDOTDIR="$test_root/zsh" SHELL=/bin/bash \
        sh "$installer" --version "$version" --bin-dir "$bin_dir" "$@"
}
install
grep -Fq '# Existing login configuration' "$test_home/.bash_profile"
for profile in "$test_home/.profile" "$test_home/.bash_profile" "$test_home/.bashrc" "$test_root/zsh/.zshenv"; do
    # zsh configuration is tested below even on runners without zsh.
    [ -f "$profile" ] || continue
    cp "$profile" "$profile.before"
done
cp "$bin_dir/selfhost" "$test_root/verified"
printf '\nprevious installation fixture\n' >> "$bin_dir/selfhost"
cp "$bin_dir/selfhost" "$test_root/original"
export CORRUPT_CHECKSUM=true
if install; then echo 'Corrupt checksum was accepted.' >&2; exit 1; fi
cmp "$bin_dir/selfhost" "$test_root/original"
export CORRUPT_CHECKSUM=false
install
cmp "$bin_dir"/selfhost.backup.* "$test_root/original"
cmp "$bin_dir/selfhost" "$test_root/verified"
install
[ "$(find "$bin_dir" -name 'selfhost.backup.*' | wc -l | tr -d ' ')" = 1 ]
for profile in "$test_home/.profile" "$test_home/.bash_profile" "$test_home/.bashrc" "$test_root/zsh/.zshenv"; do
    [ -f "$profile.before" ] || continue
    cmp "$profile" "$profile.before"
done
# A new shell loads its startup file automatically and resolves the installed
# command, including when this directory was already later in PATH.
env HOME="$test_home" PATH="$PATH:$bin_dir" EXPECTED_BIN="$bin_dir/selfhost" \
    bash --noprofile --rcfile "$test_home/.bashrc" -ic 'test "$(command -v selfhost)" = "$EXPECTED_BIN" && selfhost --version'
env HOME="$test_home" PATH="$PATH" EXPECTED_BIN="$bin_dir/selfhost" \
    sh -c '. "$HOME/.profile"; test "$(command -v selfhost)" = "$EXPECTED_BIN" && selfhost --version'
env HOME="$test_home" XDG_DATA_HOME="$test_root/data" XDG_CONFIG_HOME="$test_root/config" ZDOTDIR="$test_root/zsh" SHELL=/bin/zsh \
    sh "$installer" --version "$version" --bin-dir "$bin_dir"
if command -v zsh >/dev/null 2>&1; then
    env ZDOTDIR="$test_root/zsh" PATH="$PATH" EXPECTED_BIN="$bin_dir/selfhost" \
        zsh -c 'test "$(command -v selfhost)" = "$EXPECTED_BIN" && selfhost --version'
fi
if command -v fish >/dev/null 2>&1; then
    env HOME="$test_home" XDG_CONFIG_HOME="$test_root/config" PATH="$PATH" EXPECTED_BIN="$bin_dir/selfhost" \
        fish -c 'test (command -s selfhost) = "$EXPECTED_BIN"; and selfhost --version'
fi
# Opting out must leave both shell files and the caller's PATH untouched.
mkdir "$test_root/no-path-home"
env HOME="$test_root/no-path-home" XDG_DATA_HOME="$test_root/no-path-data" \
    sh "$installer" --version "$version" --bin-dir "$test_root/no-path-bin" --no-modify-path
[ -z "$(ls -A "$test_root/no-path-home")" ]
[ ! -e "$test_root/no-path-data" ]
# Refuse symlinks even on an ordinary update.
mkdir "$test_root/link-bin"
ln -s "$test_root/verified" "$test_root/link-bin/selfhost"
if sh "$installer" --version "$version" --bin-dir "$test_root/link-bin" --no-modify-path; then echo 'Symlink was replaced.' >&2; exit 1; fi
cmp "$test_root/verified" "$bin_dir/selfhost"
printf '%s\n' 'Installer passed: PATH startup, quoted paths, repeat installs, automatic update, corrupt download rejection, backup, symlink protection and PATH opt-out.'
