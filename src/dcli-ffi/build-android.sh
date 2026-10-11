#!/bin/bash
set -e

# Fail before invoking tools or creating build output when no key was provided.
: "${DESTINY_API_KEY:?DESTINY_API_KEY must be set to a non-empty API key}"

echo "🔨 Building dcli FFI for Android..."

# Pin Rust for reproducible builds.
export RUSTUP_TOOLCHAIN="1.83.0"
rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal
rustup target add --toolchain "$RUSTUP_TOOLCHAIN" \
    aarch64-linux-android \
    armv7-linux-androideabi \
    x86_64-linux-android \
    i686-linux-android

# Build for all Android architectures
# Output directly to the Android project's jniLibs directory
cargo ndk \
    -t arm64-v8a \
    -t armeabi-v7a \
    -t x86_64 \
    -t x86 \
    -o ../../../app/src/main/jniLibs \
    build --locked --release

echo ""
echo "✅ Android libraries built successfully!"
echo ""
echo "Libraries location:"
ls -lh ../../../app/src/main/jniLibs/*/libdcli_ffi.so
