{
  description = "smux development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rustRelease = pkgs.rust-bin.stable.latest;
        rustToolchain = rustRelease.default.override {
          extensions = [
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustfmt"
          ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          # Package with the same Rust release as the dev shell without pulling
          # editor and linting components into the build toolchain.
          cargo = rustRelease.minimal;
          rustc = rustRelease.minimal;
        };
      in
      {
        packages.smux = rustPlatform.buildRustPackage {
          pname = "smux";
          version = cargoToml.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.installShellFiles ];

          postInstall = ''
            tmpdir="$(mktemp -d)"
            "$out/bin/smux" completions zsh --dir "$tmpdir/completions"
            "$out/bin/smux" man --dir "$tmpdir/man"
            installShellCompletion --zsh "$tmpdir/completions/_smux"
            installManPage "$tmpdir"/man/*.1 "$tmpdir"/man/*.5
          '';

          meta = with pkgs.lib; {
            description = "Small Rust CLI for tmux session selection and creation";
            mainProgram = "smux";
            platforms = platforms.unix;
          };
        };

        packages.default = self.packages.${system}.smux;
        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.smux;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            pkg-config
            tmux
            fzf
            zoxide
          ];
        };
      }
    );
}
