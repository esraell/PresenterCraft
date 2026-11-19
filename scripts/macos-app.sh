#!/bin/sh
# Builds PresenterCraft and wraps it as a Mac app, target/release/PresenterCraft.app, so the
# menu bar, the Dock and Activity Monitor call it "PresenterCraft". Any arguments are passed to
# cargo (for example --no-default-features to build without NDI).
#
#   scripts/macos-app.sh
#   open target/release/PresenterCraft.app
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p pc-app "$@"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=target/release/PresenterCraft.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/presentercraft "$app/Contents/MacOS/PresenterCraft"
# NSAppSleepDisabled: macOS must not slow the app down (App Nap) while its windows are hidden;
# timed slides and timers run by the clock during a service.
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>PresenterCraft</string>
    <key>CFBundleDisplayName</key>
    <string>PresenterCraft</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.esraell.presentercraft</string>
    <key>CFBundleExecutable</key>
    <string>PresenterCraft</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSAppSleepDisabled</key>
    <true/>
</dict>
</plist>
PLIST
# A local signature, so macOS runs the app as built on this computer.
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || echo "note: could not sign the app; it may still run"
echo "built $app"
