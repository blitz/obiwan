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
  };

  outputs = inputs@{ self, nixpkgs, crane, flake-parts, advisory-db, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } ({ moduleWithSystem, ... }: {
      imports = [
        # Formatting and quality checks.
        inputs.pre-commit-hooks-nix.flakeModule
      ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      flake.nixosModules.default = moduleWithSystem (
        perSystem@{ config }:
        { ... }: {
          imports = [
            ./nix/module.nix
          ];

          services.obiwan.package = perSystem.config.packages.default;
        }
      );

      perSystem = { config, system, pkgs, lib, ... }:
        let
          craneLib = crane.lib.${system};
          src = craneLib.cleanCargoSource (craneLib.path ./ws);

          # Common arguments can be set here to avoid repeating them later
          commonArgs = {
            pname = "obiwan";

            inherit src;
          };

          # Build *just* the cargo dependencies, so we can reuse all
          # of that work (e.g. via cachix) when running in CI.
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          # Build the actual crate itself, reusing the dependency
          # artifacts from above.
          obiwan = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
        in
        {
          pre-commit.settings.hooks = {
            nixpkgs-fmt.enable = true;
            typos.enable = true;
          };

          # Only run integration tests on x86. The aarch64 runners
          # don't have KVM and the tests take too long.
          checks = lib.optionalAttrs (system == "x86_64-linux")
            (import ./nix/tests.nix {
              inherit pkgs;
              module = self.nixosModules.default;
            }) // {
            # Build the crate as part of `nix flake check` for convenience
            inherit obiwan;

            # Run clippy (and deny all warnings) on the crate source,
            # again, resuing the dependency artifacts from above.
            #
            # Note that this is done as a separate derivation so that
            # we can block the CI if there are issues here, but not
            # prevent downstream consumers from building our crate by itself.
            obiwan-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            # Audit dependencies
            obiwan-audit = craneLib.cargoAudit {
              inherit src advisory-db;
            };
          };

          packages = {
            default = obiwan;
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
