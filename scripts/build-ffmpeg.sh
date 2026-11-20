#!/bin/sh
# Builds the FFmpeg that goes inside SoftPresenter.app: the ffmpeg and ffprobe programs, which
# play the videos macOS's own player can't (MKV, WebM, AVI and others).
#
# - LGPL: configured without --enable-gpl and without --enable-nonfree, from FFmpeg's own code
#   only (no outside libraries), so its licence is LGPL 2.1 or later.
# - Linked statically, needing nothing but macOS itself.
# - Only what SoftPresenter asks of it: every decoder and file format to read, but writing only
#   the raw pictures and sound it pipes to the app, and no network.
# - The source is downloaded once from ffmpeg.org and checked against its SHA-256 (the same as
#   Homebrew's for this release) before anything is built.
#
#   scripts/build-ffmpeg.sh
#
# The programs land in target/ffmpeg/bin, with FFmpeg's licence and a note of the exact source
# in target/ffmpeg; scripts/macos-app.sh puts them in the app. A second run does nothing.
set -eu
cd "$(dirname "$0")/.."

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
URL="https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
# The oldest macOS the app runs on (LSMinimumSystemVersion in scripts/macos-app.sh).
MACOS=13.0

out=target/ffmpeg
if [ -x "$out/bin/ffmpeg" ] && [ -x "$out/bin/ffprobe" ] && [ "$(cat "$out/VERSION" 2>/dev/null)" = "$VERSION" ]; then
    echo "FFmpeg $VERSION is built in $out"
    exit 0
fi

work=$PWD/target/ffmpeg-build
mkdir -p "$work"
tarball="$work/ffmpeg-$VERSION.tar.xz"
if [ ! -f "$tarball" ]; then
    echo "downloading $URL"
    curl -fsSL -o "$tarball.part" "$URL"
    mv "$tarball.part" "$tarball"
fi
if ! echo "$SHA256  $tarball" | shasum -a 256 -c - >/dev/null 2>&1; then
    echo "error: $tarball doesn't match its SHA-256; it has been removed" >&2
    rm -f "$tarball"
    exit 1
fi

rm -rf "$work/src"
mkdir -p "$work/src"
tar -xJf "$tarball" -C "$work/src" --strip-components 1
cd "$work/src"
# The programs are taken from where they are built, not installed, so no folder of this
# computer is written into them (FFmpeg records how it was configured).
./configure \
    --enable-static --disable-shared \
    --disable-autodetect \
    --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-bzlib \
    --disable-network --disable-avdevice \
    --disable-programs --enable-ffmpeg --enable-ffprobe \
    --disable-doc --disable-debug \
    --disable-encoders --enable-encoder=rawvideo,pcm_f32le \
    --disable-muxers --enable-muxer=rawvideo,pcm_f32le \
    --disable-protocols --enable-protocol=file,pipe \
    --extra-cflags="-mmacosx-version-min=$MACOS" \
    --extra-ldflags="-mmacosx-version-min=$MACOS" \
    >"$work/configure.log"
make -j"$(sysctl -n hw.ncpu)" >"$work/make.log" 2>&1

cd - >/dev/null
rm -rf "$out"
mkdir -p "$out/bin"
for program in ffmpeg ffprobe; do
    cp "$work/src/$program" "$out/bin/$program"
    strip "$out/bin/$program"
done
# A build that needs anything outside macOS (a Homebrew library, say) won't run elsewhere.
if otool -L "$out/bin/ffmpeg" "$out/bin/ffprobe" | tail -n +2 | grep -v ':$' | grep -vE '^\s+(/usr/lib/|/System/Library/)'; then
    echo "error: the FFmpeg build needs libraries from outside macOS (above)" >&2
    exit 1
fi
if "$out/bin/ffmpeg" -hide_banner -buildconf | grep -qE 'enable-(gpl|nonfree)'; then
    echo "error: the FFmpeg build isn't LGPL" >&2
    exit 1
fi
if strings "$out/bin/ffmpeg" "$out/bin/ffprobe" | grep -q "$HOME"; then
    echo "error: the FFmpeg build has a folder of this computer written into it" >&2
    exit 1
fi
cp "$work/src/COPYING.LGPLv2.1" "$out/COPYING.LGPLv2.1"
cp "$work/src/LICENSE.md" "$out/LICENSE.md"
cat >"$out/SOURCE.txt" <<NOTE
These copies of ffmpeg and ffprobe are FFmpeg $VERSION, licensed under the GNU Lesser General
Public License, version 2.1 or later (COPYING.LGPLv2.1, and LICENSE.md for the details).

They were built, unchanged, from the source FFmpeg publishes at
  $URL
(SHA-256 $SHA256), configured with:
$("$out/bin/ffmpeg" -hide_banner -buildconf | sed -n 's/^ *--/  --/p')

FFmpeg is a trademark of Fabrice Bellard, originator of the FFmpeg project. SoftPresenter runs
these programs as separate programs; no FFmpeg code is built into SoftPresenter itself.
NOTE
echo "$VERSION" >"$out/VERSION"
echo "built FFmpeg $VERSION in $out: $(du -sh "$out/bin" | cut -f1)"
