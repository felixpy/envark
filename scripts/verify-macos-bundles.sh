#!/usr/bin/env bash
set -euo pipefail

bundle_dir="${1:?Expected the Tauri bundle directory}"

verify_app() {
  local app="$1"
  if [[ ! -f "$app/Contents/_CodeSignature/CodeResources" ]]; then
    echo "Missing application resource seal: $app" >&2
    return 1
  fi
  # Ad-hoc signing seals the whole app. It does not establish Developer ID trust.
  codesign --verify --deep --strict --verbose=2 "$app"
}

verify_app "$bundle_dir/macos/Envark.app"

scratch_dir=$(mktemp -d)
mount_dir="$scratch_dir/dmg"
mounted=false
cleanup() {
  if [[ "$mounted" == true ]]; then
    if ! hdiutil detach "$mount_dir" -quiet; then
      echo "Could not detach verification image; preserving $scratch_dir" >&2
      return 1
    fi
  fi
  rm -rf "$scratch_dir"
}
trap cleanup EXIT

archive="$bundle_dir/macos/Envark.app.tar.gz"
if [[ "${REQUIRE_UPDATER_ARCHIVE:-false}" == true || -f "$archive" ]]; then
  mkdir "$scratch_dir/updater"
  tar -xzf "$archive" -C "$scratch_dir/updater"
  verify_app "$scratch_dir/updater/Envark.app"
fi

# Inspect the copy users actually install, as well as the updater archive.
shopt -s nullglob
images=("$bundle_dir"/dmg/*.dmg)
if [[ ${#images[@]} -ne 1 ]]; then
  echo 'Expected exactly one macOS disk image' >&2
  exit 1
fi
mkdir "$mount_dir"
hdiutil attach "${images[0]}" -readonly -nobrowse -mountpoint "$mount_dir"
mounted=true
verify_app "$mount_dir/Envark.app"
hdiutil detach "$mount_dir" -quiet
mounted=false
