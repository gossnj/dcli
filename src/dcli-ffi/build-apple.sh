#!/bin/bash
set -e

# Packages macOS, iOS device, and iOS Simulator libraries.

# Fail before sourcing tools or creating build output when no key was provided.
: "${DESTINY_API_KEY:?DESTINY_API_KEY must be set to a non-empty API key}"

source "$HOME/.cargo/env"

# Use the workspace's supported Rust version for reproducible builds.
export RUSTUP_TOOLCHAIN="1.83.0"
rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal

MACOS_TARGETS=("x86_64-apple-darwin" "aarch64-apple-darwin")
IOS_TARGETS=("aarch64-apple-ios" "aarch64-apple-ios-sim" "x86_64-apple-ios")
VISIONOS_TARGETS=("aarch64-apple-visionos" "aarch64-apple-visionos-sim")

BUILD_DIR="$(pwd)/build"
mkdir -p "$BUILD_DIR"

echo "Installing Rust targets..."
for target in "${MACOS_TARGETS[@]}" "${IOS_TARGETS[@]}"; do
    rustup target add --toolchain "$RUSTUP_TOOLCHAIN" "$target"
done

# visionOS targets are placeholders; this script builds only macOS and iOS.

echo ""
echo "Building for macOS..."
for target in "${MACOS_TARGETS[@]}"; do
    echo "  Building for $target..."
    cargo build --locked --release --target "$target"
done

echo ""
echo "Building for iOS..."
for target in "${IOS_TARGETS[@]}"; do
    echo "  Building for $target..."
    cargo build --locked --release --target "$target"
done

echo ""
echo "Creating universal macOS library..."
lipo -create \
    "../target/x86_64-apple-darwin/release/libdcli_ffi.a" \
    "../target/aarch64-apple-darwin/release/libdcli_ffi.a" \
    -output "$BUILD_DIR/libdcli_ffi_macos.a"

echo ""
echo "Creating universal iOS Simulator library..."
lipo -create \
    "../target/aarch64-apple-ios-sim/release/libdcli_ffi.a" \
    "../target/x86_64-apple-ios/release/libdcli_ffi.a" \
    -output "$BUILD_DIR/libdcli_ffi_iossimulator.a"

echo ""
echo "Copying iOS device library..."
cp "../target/aarch64-apple-ios/release/libdcli_ffi.a" "$BUILD_DIR/libdcli_ffi_ios.a"

echo ""
echo "Copying header file..."
cp "dcli.h" "$BUILD_DIR/dcli.h"

echo ""
echo "Build complete! Libraries are in: $BUILD_DIR"
echo "  - libdcli_ffi_macos.a (Universal: x86_64 + arm64)"
echo "  - libdcli_ffi_ios.a (iOS Device: arm64)"
echo "  - libdcli_ffi_iossimulator.a (iOS Simulator: x86_64 + arm64)"
echo "  - dcli.h (C header file)"
