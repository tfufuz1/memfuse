{
  description = "Memfuse Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # We need specific pyo3 features if we're developing python bindings
        pythonEnv = pkgs.python3.withPackages (ps:
          with ps; [
            pip
            virtualenv
            numpy
            pytest
            # Add these if they exist in your nixpkgs, otherwise they will be ignored or fail
            # fastmcp
            # mcp
          ]);
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            (rust-bin.nightly.latest.default.override {
              extensions = ["rust-src" "rust-analyzer" "clippy" "rustfmt"];
            })
            cargo-watch
            cargo-deny
            cargo-edit

            # General utilities
            just

            # Python dependencies for pyo3
            pythonEnv
            maturin

            # System libs for build
            pkg-config
            openssl
          ];

          shellHook = ''
            export RUST_BACKTRACE=1
            export PYTHONPATH="${pythonEnv}/bin/python"
            echo "Memfuse Development Environment Loaded 🦀🐍"
          '';
        };
      }
    );
}
