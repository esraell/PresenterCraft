#!/bin/sh
# Builds SoftPresenter and wraps it as a Mac app, target/release/SoftPresenter.app, so the
# menu bar, the Dock and Activity Monitor call it "SoftPresenter". Any arguments are passed to
# cargo.
#
#   scripts/macos-app.sh
#   open target/release/SoftPresenter.app
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p sp-app "$@"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=target/release/SoftPresenter.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/softpresenter "$app/Contents/MacOS/softpresenter"
# NSAppSleepDisabled: macOS must not slow the app down (App Nap) while its windows are hidden;
# timed slides and timers run by the clock during a service.
# NSAppleEventsUsageDescription: why macOS asks to let SoftPresenter use Keynote or PowerPoint,
# which turn a presentation file into slides on import.
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>SoftPresenter</string>
    <key>CFBundleDisplayName</key>
    <string>SoftPresenter</string>
    <key>CFBundleIdentifier</key>
    <string>com.softpresenter.app</string>
    <key>CFBundleExecutable</key>
    <string>softpresenter</string>
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
    <key>NSAppleEventsUsageDescription</key>
    <string>SoftPresenter asks Keynote or PowerPoint to turn a presentation into slides when you import one.</string>
</dict>
</plist>
PLIST
# A local signature, so macOS runs the app as built on this computer.
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || echo "note: could not sign the app; it may still run"
echo "built $app"
