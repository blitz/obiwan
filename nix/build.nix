{ pkgs }:

pkgs.haskell-nix.stackProject {
  src = pkgs.haskell-nix.haskellLib.cleanGit { src = ./..; };
}
