{
  description = "Rust DevShell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.rust-overlay.follows = "rust-overlay";
      inputs.flake-utils.follows = "flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        mkRootPath = rel:
          builtins.path {
            path = "${toString ./.}/${rel}";
            name = rel;
          };
        filteredSource = let
          pathsToIgnore = [
            ".envrc"
            ".ignore"
            ".github"
            ".gitignore"
            "rust-toolchain.toml"
            "rustfmt.toml"
            "docs"
            "README.md"
            "shell.nix"
            "flake.nix"
            "flake.lock"
          ];
          ignorePaths = path: type:
            let
              inherit (nixpkgs) lib;
              # split the nix store path into its components
              components = lib.splitString "/" path;
              # drop off the `/nix/hash-source` section from the path
              relPathComponents = lib.drop 4 components;
              # reassemble the path components
              relPath = lib.concatStringsSep "/" relPathComponents;
            in lib.all (p: !(lib.hasPrefix p relPath)) pathsToIgnore;
        in builtins.path {
          name = "header-source";
          path = toString ./.;
          # filter out unnecessary paths
          filter = ignorePaths;
        };
        stdenv = if pkgs.stdenv.isLinux then pkgs.stdenv else pkgs.clangStdenv;
        rustFlagsEnv = if stdenv.isLinux then
          "$RUSTFLAGS -C link-arg=-fuse-ld=lld -C target-cpu=native -Clink-arg=-Wl,--no-rosegment"
        else
          "$RUSTFLAGS";
        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile
          ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        commonArgs = {
          inherit stdenv;
          src = filteredSource;
          # disable fetching and building of tree-sitter grammars in the helix-term build.rs
          buildInputs = [ stdenv.cc.cc.lib ];
          # disable tests
          doCheck = false;
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      in with pkgs; {

        checks = {
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          fmt = craneLib.cargoFmt commonArgs;

          doc = craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; });

          test = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });
        };

        devShells.default = mkShell {
          inputsFrom = builtins.attrValues self.checks.${system};
          nativeBuildInputs = with pkgs;
            [ lld_13 cargo-flamegraph rust-analyzer ]
            ++ (lib.optional (stdenv.isx86_64 && stdenv.isLinux)
              pkgs.cargo-tarpaulin) ++ (lib.optional stdenv.isLinux pkgs.lldb)
            ++ (lib.optional stdenv.isDarwin
              pkgs.darwin.apple_sdk.frameworks.CoreFoundation);
          shellHook = ''
            export RUST_BACKTRACE="1"
            export RUSTFLAGS="${rustFlagsEnv}"
          '';
        };
      });
  nixConfig = {
    extra-substituters = [ "https://helix.cachix.org" ];
    extra-trusted-public-keys =
      [ "helix.cachix.org-1:ejp9KQpR1FBI2onstMQ34yogDm4OgU2ru6lIwPvuCVs=" ];
  };
}
