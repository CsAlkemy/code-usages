#!/bin/sh
# Code Usages installer — downloads the right DMG for this Mac and installs
# it to /Applications. curl downloads carry no quarantine flag, so the app
# opens without Gatekeeper's unsigned-app warning.
#
#   curl -fsSL https://raw.githubusercontent.com/csalkemy/code-usages/main/install.sh | sh
set -eu

case "$(uname -m)" in
  arm64) arch=arm64 ;;
  x86_64) arch=x64 ;;
  *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

url="https://github.com/csalkemy/code-usages/releases/latest/download/code-usages-$arch.dmg"
tmp="$(mktemp -d)"
trap 'hdiutil detach "$tmp/mnt" -quiet 2>/dev/null || true; rm -rf "$tmp"' EXIT

echo "Downloading Code Usages ($arch)…"
curl -fsSL "$url" -o "$tmp/code-usages.dmg"

echo "Installing to /Applications…"
hdiutil attach "$tmp/code-usages.dmg" -nobrowse -quiet -mountpoint "$tmp/mnt"
rm -rf "/Applications/Code Usages.app"
cp -R "$tmp/mnt/Code Usages.app" /Applications/
hdiutil detach "$tmp/mnt" -quiet

open "/Applications/Code Usages.app"
echo "Done — look for the ring in your menu bar."
