# RustShare Desktop - Build and Distribution Guide

This document provides instructions for building production-ready binary packages for RustShare Desktop on macOS and Windows.

For the current macOS installation path, including first-run setup and the repo's present limitations, see [macOS client installation](./macos-client-installation.md).

The current desktop login flow is pairing-first: `rustshare-desktop login` prints a short-lived approval URL, and the user approves that device from an already authenticated RustShare web UI session.

## 1. Prerequisites

### Common Dependencies
- **Rust (Stable)**: Install via `rustup`.
- **Cargo**: Standard Rust build tool.

### macOS Specific
- **Xcode Command Line Tools**: `xcode-select --install`.

### Windows Specific
- **Visual Studio Build Tools**: Ensure the C++ desktop development workload is installed.

---

## 2. Compiling the Application

To build the optimized release version of the desktop client:

```bash
# From the root of the repository
cargo build --release -p rustshare-desktop
```

The resulting binary will be located at:
- **macOS**: `target/release/rustshare-desktop`
- **Windows**: `target/release/rustshare-desktop.exe`

---

## 3. Building Editor Plugins

### Notepad++ (Windows)
The Notepad++ plugin must be built as a `cdylib` DLL:

```bash
# Notepad++ plugin packaging was removed until the integration ships.
```
Result: `target/release/editor_npp.dll`

### Sublime Text (macOS)
The Sublime plugin is Python-based and does not require compilation. However, it should be bundled with the application distribution:
Location: `apps/editor-sublime/Packages/RustShareSync/`

---

## 4. Packaging for Distribution

### macOS (.tar.gz / DMG)
For a simple DMG, you can use `create-dmg`:

```bash
VERSION=0.3.0
mkdir -p dist/macos
rm -rf "dist/macos/rustshare-desktop-${VERSION}-macos"
mkdir -p "dist/macos/rustshare-desktop-${VERSION}-macos"
cp target/release/rustshare-desktop "dist/macos/rustshare-desktop-${VERSION}-macos/"
cp apps/desktop/CHANGELOG.md "dist/macos/rustshare-desktop-${VERSION}-macos/"
tar -czf "dist/macos/rustshare-desktop-${VERSION}-macos.tar.gz" -C dist/macos "rustshare-desktop-${VERSION}-macos"
# Sign the binary (optional but recommended for production)
# codesign -s "Developer ID Application: ..." "dist/macos/rustshare-desktop-${VERSION}-macos/rustshare-desktop"
```

This produces:
- a versioned staging directory containing the binary and `CHANGELOG.md`
- a versioned `.tar.gz` archive ready for internal distribution

### Windows (.zip / MSI)
We recommend using **WiX Toolset** or simply zipping the binary + DLL:

1. Create a `dist/windows` folder.
2. Copy `target/release/rustshare-desktop.exe`.
3. Copy `target/release/editor_npp.dll` (rename to `rustshare_npp.dll`).
4. (Optional) Provide a `config.toml` template.

---

## 5. Automation Script (Advanced)

Check the `scripts/build-all.sh` (if available) for automated cross-platform bundling.

> [!IMPORTANT]
> **Codesigning**: Production binaries MUST be signed and notarized on macOS to avoid the "Unidentified Developer" security blockade. On Windows, use a code-signing certificate (EV recommended) to avoid SmartScreen warnings.
