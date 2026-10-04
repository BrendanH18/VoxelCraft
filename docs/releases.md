# Building downloadable releases

The release packages target Windows x64 and macOS 13+ (separate Apple Silicon
and Intel DMGs). Linux is supported for source builds but has no release package.
No verified publisher signing certificates or notarization are used.

## Automated builds

`.github/workflows/release.yml` builds on native Windows and Mac runners using
Rust 1.98.1 and the committed Cargo.lock. It checks formatting, runs Clippy and
tests, generates dependency license notices, then packages and smoke-tests the
downloads. Windows uses a static C runtime so users do not need a separate
Visual C++ redistributable installation.

- Push a `release/*` branch to build downloadable workflow artifacts.
- A manual workflow run builds artifacts, without publishing.
- Push a tag matching Cargo.toml, such as `v0.2.0`, to build all three packages
  and create a **draft** GitHub Release with SHA-256 checksums and the notes from
  `packaging/RELEASE-NOTES.md`. The workflow never publishes automatically.
  Rerunning a tag build refreshes an existing draft's downloads; published
  releases are left unchanged.

Update Cargo.toml, Cargo.lock and the release notes together for each version.
Review all three packages before publishing the draft through GitHub Releases.
Do not move published version tags or replace published binaries; create a new
version for fixes so downloads remain identifiable.

## Local macOS build

Install the desired Rust target(s), then build with the deployment target set:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --release --locked --target aarch64-apple-darwin
cargo install cargo-about --locked --features cli --version 0.9.2
cargo about generate --locked --fail --target aarch64-apple-darwin \
  --output-file packaging/THIRD-PARTY-LICENSES.html packaging/licenses.hbs
bash packaging/macos/package.sh aarch64-apple-darwin
bash packaging/macos/smoke-test.sh dist/VoxelCraft-0.2.0-macos-apple-silicon.dmg
```

Replace the target with `x86_64-apple-darwin` for Intel. The smoke test must run
on a machine capable of executing the packaged architecture. The script creates
`VoxelCraft.app` (with the `voxelcraft-agent` client in `Contents/MacOS`), adds
metadata/icons/notices and an Applications shortcut, then
produces a compressed DMG in `dist/`. Its ad-hoc integrity signature supplies no
developer identity and does not satisfy Gatekeeper's publisher/notarization checks.

## Local Windows build

Use Windows with Rust's MSVC toolchain, Visual Studio C++ build tools, and
[Inno Setup 6](https://jrsoftware.org/isinfo.php). In PowerShell:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked --target x86_64-pc-windows-msvc
cargo install cargo-about --locked --features cli --version 0.9.2
cargo about generate --locked --fail --target x86_64-pc-windows-msvc `
  --output-file packaging/THIRD-PARTY-LICENSES.html packaging/licenses.hbs
./packaging/windows/package.ps1
```

The installer in `dist/` installs the game and `voxelcraft-agent.exe` for the
current user. It creates Start Menu shortcuts for the game and for hosting
agent players on loopback, optionally a desktop shortcut, and an uninstaller. Its
stable AppId lets later versions upgrade the same installation. Keep that ID.
The automated install/reinstall/uninstall test runs only on disposable CI runners.

## Player data and old saves

The default data root is `%LOCALAPPDATA%\VoxelCraft` on Windows and
`~/Library/Application Support/VoxelCraft` on Mac. Worlds live under
`saves/<name>`, settings in `saves/options.txt`, and logs in `voxelcraft.log`
and `voxelcraft.previous.log`. Installers never remove this directory.

On the first normal launch, if `<data-root>/saves` does not exist, `./saves` in
the working directory is copied into it. Originals remain untouched; the copy
is staged before it becomes visible, and failures stop startup rather than
silently creating a fresh world. Symlinks/special files require manual import.
To migrate source-build saves automatically, launch the new binary from the old
repository folder before launching the installed shortcut. After a first launch
has created the new saves directory, quit and manually copy individual world
folders instead, preserving any newer worlds.

`--data-dir <folder>` uses an isolated root and never imports legacy saves. Use
it for screenshots, development experiments and manual release testing.
World names are limited to 1–64 ASCII letters, digits, hyphens or underscores;
Windows reserved device names are rejected on all platforms.

Save downgrades are unsupported. This version reads older furnace saves,
but versions from before experience was added cannot read the new furnace
records and discard their saved contents and cooking state. Before upgrading,
quit the game and copy each complete `saves/<name>` world folder, including
its dimension subfolders, to a backup outside the active saves directory.
To return to an older build, restore its pre-upgrade backup while the game
is closed; keep the upgraded world separately rather than opening it in that build.

## Unsigned first launch

Windows SmartScreen may offer **More info → Run anyway**. Windows 11 Smart App
Control or managed-device policies may block unsigned code with no exception.
See [Microsoft's documentation](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview).

Mac users copy the app to Applications, try launching once, then use
**System Settings → Privacy & Security → Open Anyway**, when available.
See [Apple's instructions](https://support.apple.com/en-us/102445).
These packages do not disable security settings or strip quarantine metadata.

## Manual validation before publishing

CI verifies binary launch through a headless benchmark, package contents and
Windows install/reinstall/uninstall with preserved save fixtures. It does not
verify the graphics/audio/input experience or downloaded-file security prompts.
On clean personal test machines, with the actual downloaded artifacts:

1. Install and launch through Finder or the Start Menu, without Rust installed.
2. Confirm the unsigned-app prompt instructions match the device's policy.
3. Play survival and creative; check graphics, audio, input and fullscreen.
4. Change options, modify a world, quit and reopen; verify both persist. On Mac,
   check Command-Q as well as closing the window or using Save and Quit to Title.
5. Reinstall/replace the app, then uninstall; verify saves remain.
6. Test the oldest supported OS and each advertised architecture before claiming
   full compatibility. A minimum deployment target does not replace those tests.

Icons are generated from procedural geometry by
`python3 packaging/generate-icons.py`. Commit the PNG, ICO and ICNS outputs.
Dependency notices are generated for each build and are not committed.
