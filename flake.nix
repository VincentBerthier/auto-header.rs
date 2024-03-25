{
  description = "Bangk On-Chain program and Admin / BI dashboard";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    flake-utils,
    advisory-db,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };
      rustOverlay =
        pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      inherit (pkgs) lib;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustOverlay;

      src = craneLib.cleanCargoSource (craneLib.path ./.);

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit src;
        pname = "auto-header";
        version = "0.1.0";
        strictDeps = true;

        buildInputs = with pkgs;
          [
            openssl
            # Add additional build inputs here
            mold
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          mold
          bzip2
        ];

        # Additional environment variables can be set directly
        LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib;${pkgs.bzip2.out}/lib";
        # CARGO_BUILD_JOBS = 8;
      };

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      ######################################################
      ###                  Binaries                      ###
      ######################################################
      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      auto-header = craneLib.buildPackage (commonArgs
        // {
          pname = "auto-header";
          inherit cargoArtifacts;
          doCheck = false;
          nativeBuildInputs = commonArgs.nativeBuildInputs;
        });

      ######################################################
      ###               Shell aliases                    ###
      ######################################################
      aliases = ''
        alias check=\"nix flake check\" \
        && alias tests=\"cargo nextest run\"
      '';
    in {
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit auto-header;

        ######################################################
        ###               Nix flake checks                 ###
        ######################################################
        # Run clippy (and deny all warnings) on the crate source,
        # again, resuing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        auto-header-clippy = craneLib.cargoClippy (commonArgs
          // {
            pname = "auto-header-clippy";
            cargoArtifacts = auto-header;

            cargoClippyExtraArgs = "--all-features --all-targets -- --deny warnings";
          });

        # Check formatting
        auto-header-fmt = craneLib.cargoFmt {
          pname = "auto-header-fmt";
          inherit src;
        };

        # Audit dependencies
        auto-header-audit = craneLib.cargoAudit {
          pname = "auto-header-audit";
          inherit src advisory-db;
        };

        # Audit licenses
        auto-header-deny = craneLib.cargoDeny {
          pname = "auto-header-deny";
          inherit src;
        };
      };

      ######################################################
      ###                 Build packages                 ###
      ######################################################
      packages = {
        default = auto-header;
      };

      apps.default = flake-utils.lib.mkApp {drv = auto-header;};

      ######################################################
      ###                   Dev’ shell                   ###
      ######################################################
      devShells.default = craneLib.devShell {
        name = "devshell";

        # Inherit inputs from checks.
        checks = self.checks.${system};

        # Additional dev-shell environment variables can be set directly
        # CARGO_BUILD_JOBS = 8;
        LD_LIBRARY_PATH = "${pkgs.openssl.out}/lib;${pkgs.bzip2.out}/lib";
        PATH = "${pkgs.mold}/bin/mold";

        shellHook = ''
          export PATH="$HOME/.cargo/bin:$PATH"
          echo "Environnement $(basename $(pwd)) chargé" | cowsay | lolcat

          exec $SHELL -C "${aliases}"
        '';

        # Extra inputs can be added here; cargo and rustc are provided by default.
        packages = with pkgs; [
          cowsay
          lolcat
          pkg-config
          openssl

          mold # rust linker

          nodePackages.vscode-langservers-extracted # language server web
          # Cargo utilities
          cargo-bloat # check binaries size (which is fun but not terriby useful?)
          cargo-cache # cargo cache -a
          cargo-deny
          cargo-audit
          cargo-expand # for macro expension
          cargo-spellcheck # Spellcheck documentation
          # cargo-wizard
        ];
      };
    });
}
