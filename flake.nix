{
  description = "CandyPi - Lightning-paid candy dispenser for Raspberry Pi";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, crane, flake-utils, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
        
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          doCheck = false;
          
          # Let rocksdb crate build its own RocksDB for compatibility
          
          nativeBuildInputs = with pkgs; [
            rustToolchain
            cmake
            pkg-config
            perl
            clang
            llvmPackages.libclang
            llvmPackages.libcxxClang
          ];
          
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
        
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          buildPhaseCargoCommand = "cargo build --release --locked";
          doCheck = false;
          checkPhase = "";
        });
        
        candypi = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        packages.default = candypi;
        
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            pkg-config
          ];
          
          shellHook = ''
            echo "CandyPi development environment"
            echo ""
            echo "Available commands:"
            echo "  nix build                  - Build for current platform"
            echo "  nix bundle                - Create portable binary"
            echo ""
            echo "To build for Raspberry Pi on x86_64:"
            echo "  nix build --system aarch64-linux"
            echo "  nix bundle --system aarch64-linux"
            echo ""
            echo "Bundled binaries include all dependencies and work on any Linux system."
          '';
        };
      });
}