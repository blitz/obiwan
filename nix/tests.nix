{ pkgs, module }:
{
  canFetchFiles = pkgs.nixosTest {
    name = "can-fetch-files";

    nodes.server = { pkgs, lib, ... }: {
      imports = [
        module
      ];

      services.obiwan =
        let
          obiwanRoot = pkgs.runCommand "obiwan-root"
            {
              nativeBuildInputs = [
                pkgs.openssl
              ];

              # Make this a fixed-output derivation so we don't
              # needlessly rebuild it when the dependencies change.
              outputHashMode = "recursive";
              outputHashAlgo = "sha256";
              outputHash = "U26n8vF8NGrWvIW6lRkrceu6f+wxlkAy7HpOKUmyfDA=";
            } ''
            mkdir -p $out

            # We want reproducible "random" files (at least not just zeroes).
            head -c 1M /dev/zero | openssl enc -pbkdf2 -aes-128-ctr -nosalt -pass pass:12345 > $out/smallfile

            # We need a file that is larger than the typical block size (~1500 bytes) and has more blocks
            # than fits in 2^16.
            head -c 150M /dev/zero | openssl enc -pbkdf2 -aes-128-ctr -nosalt -pass pass:12345 > $out/largefile
          '';
        in
        {
          enable = true;

          listenAddress = "0.0.0.0";
          openFirewall = true;

          root = "${obiwanRoot}";
        };
    };

    nodes.client = { pkgs, lib, ... }: {

      # The TFTP server will send us packets on a new UDP port.
      networking.firewall.enable = false;

      environment.systemPackages = [
        pkgs.inetutils # tftp
      ];
    };

    testScript = ''
      server.start()
      server.wait_for_unit("network-online.target", timeout = 120)
      server.wait_for_unit("obiwan.service")

      client.start()
      client.wait_for_unit("network-online.target", timeout = 120)

      client.succeed("echo get smallfile | tftp server", timeout = 120)

      client.succeed("echo get largefile | tftp server", timeout = 120)

      # TODO Check whether file is intact.

      # TODO Check it again with pkgs.atftpd
    '';
  };
}
