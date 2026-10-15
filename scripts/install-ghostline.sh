#!/bin/sh
set -eu

repository='AduAkorful/ghostline'
archive='ghostline-x86_64-unknown-linux-gnu.tar.gz'

if [ "$(uname -s)" != 'Linux' ] || [ "$(uname -m)" != 'x86_64' ]; then
  echo 'Prebuilt Ghostline downloads currently support Linux x86-64 only.' >&2
  echo "For other platforms, build from source: https://github.com/$repository#running-the-tests" >&2
  exit 1
fi

if [ -n "${GHOSTLINE_VERSION:-}" ]; then
  case "$GHOSTLINE_VERSION" in
    v[0-9]*)
      case "$GHOSTLINE_VERSION" in
        *[!A-Za-z0-9._+-]*) echo 'GHOSTLINE_VERSION contains invalid characters.' >&2; exit 1 ;;
      esac
      release_url="https://github.com/$repository/releases/download/$GHOSTLINE_VERSION"
      ;;
    *) echo 'GHOSTLINE_VERSION must be a release tag such as v0.1.0.' >&2; exit 1 ;;
  esac
else
  release_url="https://github.com/$repository/releases/latest/download"
fi

if [ -z "${HOME:-}" ]; then
  echo 'HOME is not set; choose GHOSTLINE_INSTALL_DIR explicitly.' >&2
  exit 1
fi
install_dir=${GHOSTLINE_INSTALL_DIR:-"$HOME/.local/bin"}
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' 0 HUP INT TERM

curl --fail --location --silent --show-error --retry 3 \
  "$release_url/$archive" -o "$tmp_dir/$archive"
curl --fail --location --silent --show-error --retry 3 \
  "$release_url/SHA256SUMS" -o "$tmp_dir/SHA256SUMS"

expected_hash=$(awk -v file="$archive" '$2 == file { print $1; found = 1 } END { if (!found) exit 1 }' "$tmp_dir/SHA256SUMS") || {
  echo "No checksum for $archive was published with this release." >&2
  exit 1
}
printf '%s  %s\n' "$expected_hash" "$tmp_dir/$archive" | sha256sum --check --status || {
  echo 'The Ghostline archive checksum did not match; refusing to install it.' >&2
  exit 1
}

mkdir -p "$tmp_dir/unpacked"
tar -xzf "$tmp_dir/$archive" -C "$tmp_dir/unpacked"
for executable in ghostline ghostline-mcp ghostline-paid-api; do
  if [ ! -f "$tmp_dir/unpacked/$executable" ]; then
    echo "The release archive is missing $executable; refusing to install it." >&2
    exit 1
  fi
done

mkdir -p "$install_dir"
for executable in ghostline ghostline-mcp ghostline-paid-api; do
  install -m 0755 "$tmp_dir/unpacked/$executable" "$install_dir/$executable"
done

printf 'Installed Ghostline executables to %s\n' "$install_dir"
case ":${PATH:-}:" in
  *":$install_dir:"*) ;;
  *) printf 'Add this directory to PATH to run the commands directly: %s\n' "$install_dir" ;;
esac
printf 'Try: %s --help\n' "$install_dir/ghostline"
