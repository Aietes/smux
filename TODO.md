# TODO

## Publishing

### Project metadata

- Optionally add `documentation` metadata in `Cargo.toml`.

### crates.io

- Configure the `crates-io` GitHub Actions environment.
- Add the `CARGO_REGISTRY_TOKEN` environment secret.
- Optionally require manual approval for that environment.
- Verify `cargo install smux-cli` works from crates.io after the publish propagates.

### GitHub Releases

- Create the first version tag, e.g. `v0.1.0`.
- Push the tag so the release workflow runs.
- Verify release artifacts are attached:
  - platform archives
  - `SHA256SUMS`
  - man pages
  - zsh completions
- Review archive names and install instructions against the published release.

### Homebrew

- Wait for the first tagged GitHub release.
- Use the release archive URL and checksum from `SHA256SUMS`.
- Create a Homebrew formula or tap entry for `smux`.
- Add a minimal formula test such as `smux --help`.
- Validate with:
  - `brew install --build-from-source`
  - `brew test smux`
  - `brew audit`
- Submit the formula PR or publish the tap.

### nixpkgs

- Wait for the first tagged release source.
- Translate the current flake package logic into a nixpkgs derivation.
- Compute the nixpkgs `cargoHash`.
- Ensure the nixpkgs package installs:
  - binary
  - man pages
  - zsh completions
- Verify the package locally with a nixpkgs-style build.
- Submit the nixpkgs PR.

### Documentation

- Replace “planned/pending” publication notes with real install commands once crates.io and package managers are live.
- Add release-specific install examples once GitHub release artifacts are published.
