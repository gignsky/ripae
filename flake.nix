{
  description = "Ripae -- org commodity-run ledger";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;
        commonArgs = {
          inherit src;
          strictDeps = true;
          buildInputs = [ ];
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        ripae = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );

        # `dockerTools.buildLayeredImage` -- the Nix path from §6. Not
        # exercised in the sandbox this was written in (no outbound access
        # to build/pull the base layers there); see README.md "Status".
        container = pkgs.dockerTools.buildLayeredImage {
          name = "ripae";
          tag = "0.1.0";
          contents = [
            ripae
            pkgs.cacert
          ];
          config = {
            Entrypoint = [ "${ripae}/bin/ripae" ];
            ExposedPorts = {
              "8080/tcp" = { };
            };
            Volumes = {
              "/data" = { };
            };
            WorkingDir = "/";
          };
          extraCommands = ''
            mkdir -p static
            cp -r ${./static}/* static/
          '';
        };
      in
      {
        packages = {
          default = ripae;
          inherit container;
        };

        apps.default = flake-utils.lib.mkApp { drv = ripae; };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ ripae ];
          packages = [
            rustToolchain
            pkgs.sqlx-cli
          ];
        };

        checks.ripae = ripae;
      }
    );
}
