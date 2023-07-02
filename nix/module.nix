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
      description = "The directory that will be shared via TFTP";
    };

    openFirewall = mkOption {
      default = false;
      type = types.bool;
      description = "Open firewall ports";
    };

    listenAddress = mkOption {
      description = "Listen on this IP";
      default = "127.0.0.1";
      type = types.str;
    };

    listenPort = mkOption {
      description = "Listen on this port";
      default = 69;
      type = types.int;
    };

    extraOptions = mkOption {
      description = "Additional command-line arguments to obiwan";
      default = [ ];
      type = types.listOf types.str;
    };
  };

  config = mkIf cfg.enable {

    networking.firewall.allowedUDPPorts = mkIf cfg.openFirewall [ cfg.listenPort ];

    systemd.services.obiwan = {
      description = "Obiwan TFTP Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      # This is currently not compatible with DynamicUser.
      #
      # confinement = {
      #   enable = true;
      #   binSh = null;
      # };

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/obiwan -l '${cfg.listenAddress}:${toString cfg.listenPort}' '${cfg.root}' ${lib.concatStringsSep " " cfg.extraOptions}";

        # It would be nice if we could use this. Prevents us from
        # binding to the server port.
        #
        # DynamicUser = true;

        # Obiwan does this on its own, but it can't hurt.
        NoNewPrivileges = true;

        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];

        # These prevent binding to the server port.
        #
        # PrivateDevices = true;
        # PrivateUsers = true;

        ProtectClock = true;
        ProtectHostname = true;
        PrivateTmp = true;

        # Mount everything read-only except /dev, /proc, /sys.
        ProtectSystem = "strict";

        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        SystemCallArchitectures = "native";
        MemoryDenyWriteExecute = true;

        # This somehow fails to do what I think it does. Instead of limiting capabilities, we get none?
        #
        # CapabilityBoundingSet = [
        #   "CAP_SYS_CHROOT"
        #   "CAP_SET_UID"

        #   # We could get rid of this, if we let systemd open our server socket.
        #   "CAP_NET_BIND_SERVICE"
        # ];
      };
    };
  };
}
