# Contributing to VoxelCraft

Bug reports, focused fixes, documentation improvements, and feature ideas are welcome.
For a larger change, open an issue first to discuss the approach. Search existing
issues and pull requests before starting.

## Set up

Install a recent stable Rust toolchain with `rustfmt` and `clippy`. VoxelCraft uses
Rust edition 2024. Playing the game requires a GPU supported by wgpu's Metal,
Vulkan, or DirectX 12 backends.

On Debian/Ubuntu, install the native development dependencies used in CI:

```sh
sudo apt install libasound2-dev libudev-dev pkg-config
```

From your checkout:

```sh
rustup component add rustfmt clippy
cargo run --release
```

See the [README](README.md) for setup and controls, the
[development guide](docs/development.md) for command-line examples, and the
[architecture notes](docs/architecture.md#code-layout) for the code layout.
Worlds are saved in the [per-user data folder](docs/releases.md#player-data-and-old-saves).
Use `--data-dir target/playtest` for isolated testing;
`--new` ignores an existing save and can replace it when the game saves.

## Make and check a change

Keep pull requests focused and explain the behavior they change. Add or update
tests when changing rules that can be checked without a GPU, and describe manual
checks for graphics, input, or audio changes.

Run the same checks as CI before submitting:

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
cargo run --release -- --bench --rd 4
```

Use `cargo fmt` to apply formatting. The headless benchmark checks generation and
meshing without opening a window. For rendering or performance changes, also
compare `cargo run --release -- --bench-render --rd 8` before and after on the
same machine, and report your GPU, resolution, seed, and render distance.

Include screenshots for visible changes when useful. Avoid committing local
saves, build output, or generated sound exports.

## Report an issue

Use the bug report or feature request form. A useful bug report includes the
exact launch command, commit, OS, GPU, Rust version, and reproduction steps.
For world-specific bugs, include the seed, coordinates, render distance, and
whether you used an existing save. For crashes, include terminal output; run
with `RUST_BACKTRACE=1` to capture a Rust panic backtrace.

## License

VoxelCraft is dual licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE). Unless you explicitly state otherwise, contributions
intentionally submitted for inclusion are dual licensed on the same terms, as
described in the README.
