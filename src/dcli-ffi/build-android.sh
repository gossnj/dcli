#!/bin/bash
set -e

echo "🔨 Building dcli FFI for Android..."

# Override the rust-toolchain.toml which pins to 1.70.0
# We need a newer version for Android dependencies
export RUSTUP_TOOLCHAIN=stable

# API key required at compile time by dcli
# This is a fallback; the actual key is passed at runtime
export DESTINY_API_KEY="12197628758a4526a9c86afe4c4ad826"

# Build for all Android architectures
# Output directly to the Android project's jniLibs directory
cargo ndk \
    -t arm64-v8a \
    -t armeabi-v7a \
    -t x86_64 \
    -t x86 \
    -o ../../../app/src/main/jniLibs \
    build --release

echo ""
echo "✅ Android libraries built successfully!"
echo ""
echo "Libraries location:"
ls -lh ../../../app/src/main/jniLibs/*/libdcli_ffi.so
