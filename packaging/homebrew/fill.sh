#!/bin/sh
# Render the Homebrew formula for one Forge release.
#
#   packaging/homebrew/fill.sh <version> <SHA256SUMS.txt> [template] > forge.rb
#
# <version> may carry a leading "v" (0.10.0 or v0.10.0). <SHA256SUMS.txt> is
# the file the release workflow attaches to the GitHub release
# (`sha256sum` format: "<hex>  forge-v<version>-<target>.tar.gz").
# Fails if any target the formula needs is missing from the sums file or if
# a placeholder is left unfilled.
set -eu

usage() {
    echo "usage: $0 <version> <SHA256SUMS.txt> [template]" >&2
    exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
version=${1#v}
sums=$2
template=${3:-$(dirname "$0")/forge.rb.tmpl}

case $version in
    '' | *[!0-9A-Za-z.+-]*)
        echo "error: invalid version '$1'" >&2
        exit 2
        ;;
esac
[ -r "$sums" ] || { echo "error: cannot read $sums" >&2; exit 2; }
[ -r "$template" ] || { echo "error: cannot read $template" >&2; exit 2; }

# Targets shipped as tar.gz by .github/workflows/release.yml (Windows ships a
# .zip and has no Homebrew formula).
targets="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu"

script=""
for target in $targets; do
    asset="forge-v${version}-${target}.tar.gz"
    # sha256sum writes "<hex>  <name>" (or "<hex> *<name>" in binary mode).
    sha=$(awk -v a="$asset" '$2 == a || $2 == "*" a { print $1 }' "$sums")
    case $sha in
        '')
            echo "error: no checksum for $asset in $sums" >&2
            exit 1
            ;;
        *[!0-9a-f]*)
            echo "error: malformed checksum for $asset: $sha" >&2
            exit 1
            ;;
    esac
    if [ ${#sha} -ne 64 ]; then
        echo "error: checksum for $asset is not 64 hex characters" >&2
        exit 1
    fi
    script="${script}s|@SHA_${target}@|${sha}|g;"
done
script="${script}s|@VERSION@|${version}|g"

out=$(sed "$script" "$template")
if printf '%s\n' "$out" | grep -q '@[A-Za-z_0-9-]*@'; then
    echo "error: unfilled placeholders remain:" >&2
    printf '%s\n' "$out" | grep -n '@[A-Za-z_0-9-]*@' >&2
    exit 1
fi
printf '%s\n' "$out"
