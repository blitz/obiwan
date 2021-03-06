{ sources ? import ./nix/sources.nix, nixpkgs ? sources.nixpkgs
, pkgs ? import nixpkgs (import sources.haskell) }:

let
  obiwanComponents =
    (import ./nix/build.nix { inherit pkgs; }).obiwan.components;
in rec {
  obiwan = obiwanComponents.exes.obiwan;

  obiwanModule = import ./nix/module.nix;

  integration-test = pkgs.callPackage ./nix/test.nix {
    inherit obiwan;
    inherit obiwanModule;
  };

  # This currently fails to build due to "ghc: could not execute: hspec-discover"
  #test = obiwanComponents.tests.obiwan-test;
}
