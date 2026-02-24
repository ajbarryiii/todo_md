{
  description = "Dev environment for markdown todo CLI (Rust) and mobile app (React Native)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
      in {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            # Rust toolchain + quality-of-life tools
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            cargo-watch

            # CLI/runtime dependencies
            pkg-config
            openssl

            # Helpful utilities for scripts and API/dev tooling
            git
            jq
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # Common iOS dependency for React Native projects
            cocoapods
          ];

          shellHook = ''
            export RUST_BACKTRACE=1
            export CARGO_HOME="${toString ./.}/.cargo"
            export PATH="$CARGO_HOME/bin:$PATH"

            # React Native Metro can hit file watcher limits on Linux.
            # If needed, bump inotify limits outside nix shell.

            echo "Rust + React Native development shell ready."
            echo "- Rust: $(rustc --version)"
            echo "- Cargo: $(cargo --version)"
          '';
        };
      });
}

