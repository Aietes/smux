{
  description = "smux development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
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
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustfmt"
          ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.smux = rustPlatform.buildRustPackage {
          pname = "smux";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.installShellFiles ];

          postInstall = ''
            tmpdir="$(mktemp -d)"
            "$out/bin/smux" completions zsh --dir "$tmpdir/completions"
            "$out/bin/smux" man --dir "$tmpdir/man"
            installShellCompletion --zsh "$tmpdir/completions/_smux"
            installManPage "$tmpdir"/man/*.1
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
