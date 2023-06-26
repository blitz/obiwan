{ pkgs, module }:
{
  canFetchFiles = import ./can-fetch-files.nix { inherit pkgs module; };
}
