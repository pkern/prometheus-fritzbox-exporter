{ config, lib, pkgs, ... }:

let
  cfg = config.services.prometheus.exporters.pomfritz;
in {
  options.services.prometheus.exporters.pomfritz = with lib; {
    enable = mkEnableOption "pomfritz Fritzbox exporter";
  };

  config = lib.mkIf cfg.enable {
    systemd.services."prometheus-pomfritz-exporter" = with lib; {
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig.ExecStart = ''
        ${pkgs.pomfritz}/bin/pomfritz
      '';

      serviceConfig.Restart = mkDefault "always";
      serviceConfig.PrivateTmp = mkDefault true;
      serviceConfig.WorkingDirectory = mkDefault /tmp;
      serviceConfig.DynamicUser = mkDefault true;
      serviceConfig.User = mkDefault conf.user;
      serviceConfig.Group = conf.group;
      # Hardening
      serviceConfig.CapabilityBoundingSet = mkDefault [ "" ];
      serviceConfig.DeviceAllow = [ "" ];
      serviceConfig.LockPersonality = true;
      serviceConfig.MemoryDenyWriteExecute = true;
      serviceConfig.NoNewPrivileges = true;
      serviceConfig.PrivateDevices = mkDefault true;
      serviceConfig.ProtectClock = mkDefault true;
      serviceConfig.ProtectControlGroups = true;
      serviceConfig.ProtectHome = true;
      serviceConfig.ProtectHostname = true;
      serviceConfig.ProtectKernelLogs = true;
      serviceConfig.ProtectKernelModules = true;
      serviceConfig.ProtectKernelTunables = true;
      serviceConfig.ProtectSystem = mkDefault "strict";
      serviceConfig.RemoveIPC = true;
      serviceConfig.RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];
      serviceConfig.RestrictNamespaces = true;
      serviceConfig.RestrictRealtime = true;
      serviceConfig.RestrictSUIDSGID = true;
      serviceConfig.SystemCallArchitectures = "native";
      serviceConfig.UMask = "0077";
    };
  };
}
