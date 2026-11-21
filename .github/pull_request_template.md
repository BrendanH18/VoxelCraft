## What changes?

<!-- Describe the problem and resulting behavior. Link a related issue if applicable. -->

## Validation

<!-- List the checks you ran and any manual gameplay, graphics, or audio checks.
     Explain checks you could not run. Include screenshots for visible changes and
     before/after measurements with hardware/settings for performance changes. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --profile ci --all-targets -- -D warnings`
- [ ] `cargo test --profile ci`
- [ ] `cargo run --profile ci -- --bench --rd 4`
