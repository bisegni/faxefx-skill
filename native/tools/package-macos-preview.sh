#!/bin/sh
# Developer preview only: audio interpreter/provider runtimes are not bundled yet.
set -eu
cd "$(dirname "$0")/../.."
if [ "$(uname -s)" != Darwin ]; then
    echo 'This preview packager requires macOS.' >&2
    exit 1
fi
cargo build -p axe-desktop --release --locked
app='dist/Axe Studio Development.app'
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/axefx-live"
cp target/release/axe-desktop "$app/Contents/MacOS/axe-desktop"
cp native/resources/axefx-live/SKILL.md "$app/Contents/Resources/axefx-live/SKILL.md"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Axe Studio Development</string>
<key>CFBundleDisplayName</key><string>Axe Studio Development</string>
<key>CFBundleIdentifier</key><string>org.axestudio.development</string>
<key>CFBundleExecutable</key><string>axe-desktop</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
printf '%s\n' 'INCOMPLETE UNSIGNED DEVELOPMENT PREVIEW. Not an acceptance candidate.' > "$app/Contents/Resources/STATUS.txt"
git rev-parse HEAD > "$app/Contents/Resources/COMMIT.txt"
shasum -a 256 "$app/Contents/MacOS/axe-desktop" > dist/axe-studio-development.sha256
printf 'Preview: %s\n' "$app"
