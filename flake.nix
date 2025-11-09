{
  description = "CandyPi - Lightning-paid candy dispenser for Raspberry Pi";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        
        # Use pkgsCross for proper cross-compilation
        pkgsCross = pkgs.pkgsCross.aarch64-multiplatform-musl;
        
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
          targets = [ "aarch64-unknown-linux-musl" ];
        };
        
        # Create custom rust platform for cross-compilation
        rustPlatform = pkgsCross.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        
        # Build the cross-compiled binary using buildRustPackage
        candypi = rustPlatform.buildRustPackage rec {
          pname = "candypi";
          version = "0.1.0";
          
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          nativeBuildInputs = [ rustToolchain ];
          
          # Configure cross-compilation
          CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
          
          # Use static linking
          RUSTFLAGS = "-C target-feature=+crt-static";
          
          # Skip tests for cross-compilation
          doCheck = false;
          
          postInstall = ''
            # Strip the binary to reduce size
            ${pkgsCross.stdenv.cc.targetPrefix}strip $out/bin/candypi || true
          '';
        };
        
      in
      {
        packages = {
          default = candypi;
          candypi-arm64 = candypi;
        };
        
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            pkg-config
          ];
          
          shellHook = ''
            echo "CandyPi development environment"
            echo ""
            echo "To build for ARM64 (Raspberry Pi):"
            echo "  nix build .#candypi-arm64"
            echo ""
            echo "The resulting binary will be in: result/bin/candypi"
            echo ""
            echo "To deploy to your Raspberry Pi:"
            echo "  scp result/bin/candypi pi@your-pi-address:/home/pi/"
            echo "  ssh pi@your-pi-address"
            echo "  sudo ./candypi"
            echo ""
            echo "Note: The binary is statically linked and standalone."
          '';
        };
      });
}