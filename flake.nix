{
  description = "Obiwan TFTP Server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";

    flake-utils.url = "github:numtide/flake-utils";

    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    pre-commit-hooks-nix = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.flake-compat.follows = "flake-compat";
    };

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.flake-compat.follows = "flake-compat";
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };
  };

  outputs = inputs@{ self, nixpkgs, crane, flake-parts, fenix, advisory-db, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } ({ moduleWithSystem, ... }: {
      imports = [
        # Formatting and quality checks.
        inputs.pre-commit-hooks-nix.flakeModule
      ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = { config, system, pkgs, ... }:
        let
          craneLib = crane.lib.${system};
          src = craneLib.cleanCargoSource (craneLib.path ./ws);

          # Common arguments can be set here to avoid repeating them later
          commonArgs = {
            pname = "obiwan";

            inherit src;
          };

          craneLibLLvmTools = craneLib.overrideToolchain
            (fenix.packages.${system}.complete.withComponents [
              "cargo"
              "llvm-tools"
              "rustc"
            ]);

          # Build *just* the cargo dependencies, so we can reuse all
          # of that work (e.g. via cachix) when running in CI
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          # Build the actual crate itself, reusing the dependency
          # artifacts from above.
          my-crate = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
        in {
          pre-commit = {
            settings = {

              hooks = {
                nixpkgs-fmt.enable = true;
                typos.enable = true;
              };
            };
          };

          checks = {
            # Build the crate as part of `nix flake check` for convenience
            inherit my-crate;

            # Run clippy (and deny all warnings) on the crate source,
            # again, resuing the dependency artifacts from above.
            #
            # Note that this is done as a separate derivation so that
            # we can block the CI if there are issues here, but not
            # prevent downstream consumers from building our crate by itself.
            my-crate-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            # Audit dependencies
            my-crate-audit = craneLib.cargoAudit {
              inherit src advisory-db;
            };
          };

          packages = {
            default = my-crate;
            my-crate-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs // {
              inherit cargoArtifacts;
            });
          };

          devShells.default = pkgs.mkShell {
            shellHook = ''
              ${config.pre-commit.installationScript}
            '';

            inputsFrom = [
              config.packages.default
            ];
          };
        };
    });
}
