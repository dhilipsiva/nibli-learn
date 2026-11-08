{
  description = "Nibli Learn development and verification environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.stable."1.96.1".default.override {
          extensions = [ "clippy" "rustfmt" "rust-src" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
        # The asset linker must match the application's Dioxus version.
        dx = assert pkgs.dioxus-cli.version == "0.7.3"; pkgs.dioxus-cli;
      in {
        devShells.default = pkgs.mkShell ({
          packages = [ rust dx pkgs.wasm-pack pkgs.binaryen pkgs.just pkgs.nodejs
            pkgs.python3 pkgs.pkg-config pkgs.openssl ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.chromium ];
          PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
        } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          BROWSER_EXECUTABLE = "${pkgs.chromium}/bin/chromium";
        });
      });
}
