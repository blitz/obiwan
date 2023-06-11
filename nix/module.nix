{ lib, config, pkgs, ... }:
with lib;
let
  cfg = config.services.obiwan;
in
{
  options.services.obiwan = {
    enable = mkEnableOption "Obiwan TFTP server";

    package = mkOption {
      type = types.package;
      default = pkgs.obiwan;
      description = "Obiwan TFTP server package";
    };

    root = mkOption {
      default = "/srv/tftp";
      type = types.path;
      description = "The directory that will be shared via TFTP"
        };

      listenAddress = mkOption {
        description = "Listen on this IP";
        default = "127.0.0.1";
        type = types.string;
      };

      listenPort = mkOption {
        description = "Listen on this port";
        default = 67;
        type = types.integer;
      };
    };

    config = mkIf cfg.enable {
      systemd.services.atftpd = {
        description = "Obiwan TFTP Server";
        after = [ "network.target" ];
        wantedBy = [ "multi-user.target" ];

        serviceConfig = {
          ExecStart = "${cfg.package}/bin/obiwan -l '${cfg.listenAddress}:${cfg.listenPort}' '${cfg.root}'";

          DynamicUser = true;
          NoNewPrivileges = true;
          RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];
          PrivateDevices = true;
          PrivateUsers = true;
          ProtectClock = true;
          ProtectControlGroups = true;
          ProtectHome = true;
          ProtectKernelLogs = true;
          ProtectKernelModules = true;
          ProtectKernelTunables = true;
          SystemCallArchitectures = "native";

          confinement = {
            enable = true;
            binSh = null;
          };

          AmbientCapabilities = [
            "CAP_SYS_CHROOT"
          ] ++ optional (cfg.listenPort < 1024) "CAP_NET_BIND_SERVICE" l
            };
        };
      };
    }
