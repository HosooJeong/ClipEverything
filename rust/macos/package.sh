#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    echo "Use a macOS Apple Silicon host with the Xcode command-line tools." >&2
    exit 1
fi

rust_target=aarch64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=14.0
export CARGO_TARGET_DIR="$PWD/target"
export LC_ALL=en_US.UTF-8
stage="$CARGO_TARGET_DIR/macos-beta-image"
app="$stage/ClipEverything.app"
contents="$app/Contents"
out="$PWD/dist/macos-beta"
dmg="$out/ClipEverything-macOS-arm64.dmg"
build_log="$CARGO_TARGET_DIR/macos-library-build.log"

mkdir -p "$contents/MacOS" "$contents/Resources" "$out"
cp macos/Info.plist "$contents/Info.plist"
cp ../LICENSE "$contents/Resources/LICENSE.txt"
cp THIRD-PARTY-NOTICES.txt "$contents/Resources/THIRD-PARTY-NOTICES.txt"
if [[ ! -e "$stage/Applications" ]]; then
    ln -s /Applications "$stage/Applications"
fi
plutil -lint "$contents/Info.plist"

cargo rustc --locked --lib --release --target "$rust_target" --crate-type staticlib -- \
    --print native-static-libs 2>&1 | tee "$build_log"
native_libraries=$(sed -n 's/^note: native-static-libs: //p' "$build_log" | tail -1)
if [[ -z "$native_libraries" ]]; then
    echo "Could not determine the Rust static library's native dependencies." >&2
    exit 1
fi
read -r -a rust_link_flags <<< "$native_libraries"

xcrun swiftc -O -parse-as-library -target arm64-apple-macos14.0 \
    -import-objc-header macos/Bridge.h -framework AppKit -framework SwiftUI -framework Carbon \
    macos/ClipEverything.swift "$CARGO_TARGET_DIR/$rust_target/release/libclip_everything.a" \
    "${rust_link_flags[@]}" -o "$contents/MacOS/ClipEverything"

codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"
"$contents/MacOS/ClipEverything" --smoke-test
hdiutil create -ov -volname "ClipEverything Beta" -srcfolder "$stage" -format UDZO "$dmg"
hdiutil verify "$dmg"
(
    cd "$out"
    shasum -a 256 ClipEverything-macOS-arm64.dmg > SHA256SUMS.txt
    shasum -a 256 -c SHA256SUMS.txt
)
