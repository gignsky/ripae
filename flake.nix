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

        # cleanCargoSource keeps only Rust/Cargo files; askama templates,
        # sqlx::migrate! migrations, and CHANGELOG.md (include_str!) are also
        # read at compile time.
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            (craneLib.fileset.commonCargoSources ./.)
            ./templates
            ./migrations
            ./CHANGELOG.md
          ];
        };
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

        # `dockerTools.buildLayeredImage` -- the Nix path from §6.
        container = pkgs.dockerTools.buildLayeredImage {
          name = "ripae";
          tag = "0.2.0";
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
            # nixpkgs' sqlx-cli 0.9.0 links -lsqlite3 but omits sqlite from
            # buildInputs, so it fails to build; add it until upstream fixes it.
            (pkgs.sqlx-cli.overrideAttrs (old: {
              buildInputs = (old.buildInputs or [ ]) ++ [ pkgs.sqlite ];
            }))
          ];
        };

        checks.ripae = ripae;
      }
    );
}
