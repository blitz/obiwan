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
        RestrictRealtime = true;
        LockPersonality = true;
        RestrictSUIDSGID = true;
        RestrictNamespaces = true;
        ProcSubset = "pid";
        ProtectProc = "invisible";
        Umask = "077";

        SystemCallFilter = [
          "~@clock"
          "~@cpu-emulation"
          "~@debug"
          "~@module"
          "~@obsolete"
          "~@raw-io"
          "~@reboot"
          "~@resources"
          "~@swap"
          "~@sync"
        ];

        CapabilityBoundingSet = [
          "~CAP_AUDIT_CONTROL"
          "~CAP_AUDIT_READ"
          "~CAP_AUDIT_WRITE"
          "~CAP_BLOCK_SUSPEND"
          "~CAP_CHOWN"
          "~CAP_FSETID"
          "~CAP_IPC_LOCK"
          "~CAP_KILL"
          "~CAP_LEASE"
          "~CAP_LINUX_IMMUTABLE"
          "~CAP_MAC_ADMIN"
          "~CAP_MAC_OVERRIDE"
          "~CAP_MKNOD"
          "~CAP_NET_ADMIN"
          "~CAP_NET_RAW"
          "~CAP_SETFCAP"
          "~CAP_SYSLOG"
          "~CAP_SYS_ADMIN"
          "~CAP_SYS_BOOT"
          "~CAP_SYS_NICE"
          "~CAP_SYS_PACCT"
          "~CAP_SYS_PTRACE"
          "~CAP_SYS_RAWIO"
          "~CAP_SYS_RESOURCE"
          "~CAP_SYS_TTY_CONFIG"
        ];

        # Instead of the above, I would rather build an allow-list,
        # but this doesn't grant any capabilities?
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
