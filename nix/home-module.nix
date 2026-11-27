# Home Manager module: `scree web` as a launchd agent (macOS).
#
# The server listens on a free port chosen at start. The CLI finds it over the
# control socket beside the database (`scree web status`).
self:
{ config, lib, pkgs, ... }:
let
  cfg = config.services.scree-web;
  logs = "${config.home.homeDirectory}/Library/Logs";
in
{
  options.services.scree-web = {
    enable = lib.mkEnableOption "the scree web UI as a launchd agent";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "scree.packages.\${system}.default";
      description = "The scree package to run. Install the same one for the CLI.";
    };

    extraArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "--bind" "0.0.0.0" "--i-know-this-is-open" ];
      description = ''
        Further `scree web` flags. A non-loopback `--bind` is refused without
        `--i-know-this-is-open` and has no login: anyone who can reach the port
        can read and change tickets. Do not pass `--port`; the module already
        does.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = pkgs.stdenv.hostPlatform.isDarwin;
        message = "services.scree-web runs under launchd and supports only macOS";
      }
    ];

    launchd.agents.scree-web = {
      enable = true;
      config = {
        ProgramArguments = [ "${cfg.package}/bin/scree" "web" "--port" "0" ] ++ cfg.extraArgs;
        RunAtLoad = true;
        KeepAlive = true;
        EnvironmentVariables = {
          # launchd does not pass the shell's XDG_DATA_HOME, and the server and
          # CLI find each other through the database path.
          SCREE_DB = "${config.xdg.dataHome}/scree/scree.db";
          # Linking commits runs git, and launchd's PATH has no nix profile.
          PATH = "${lib.makeBinPath [ pkgs.git ]}:/usr/bin:/bin";
        };
        StandardOutPath = "${logs}/scree-web.log";
        StandardErrorPath = "${logs}/scree-web.err.log";
      };
    };
  };
}
