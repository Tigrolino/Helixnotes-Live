{
  description = "HelixNotes (local Markdown notes app) with Nix flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };

        cargoToml = builtins.fromTOML (builtins.readFile ./src-tauri/Cargo.toml);
        pname = cargoToml.package.name;
        version = cargoToml.package.version;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          inherit pname version;
          src = ./.;

          cargoHash = "sha256-P0iXImWYKYPxVj7q2JSuDmd8JvjcKW4DzFUzXeUQZPk=C";

          pnpmDeps = pkgs.fetchPnpmDeps {
            inherit pname version;
            src = ./.;
            fetcherVersion = 4;
            hash = "sha256-odUNKO2D50DG+VpuoTEo5FLMYy5pQSHQBtfHJrhJt78=";
          };

          nativeBuildInputs = with pkgs; [
            cargo-tauri.hook

            pnpmConfigHook
            pnpm
            nodejs

            pkg-config
            jq
            moreutils
            wrapGAppsHook3
          ];

          buildInputs =
            with pkgs;
            lib.optionals stdenv.hostPlatform.isLinux [
              webkitgtk_4_1
              libayatana-appindicator
            ];

          cargoRoot = "src-tauri";
          buildAndTestSubdir = self.packages.${system}.default.cargoRoot;

          # Deactivate the upstream update mechanism
          postPatch = ''
            jq '
              .bundle.createUpdaterArtifacts = false |
              .plugins.updater = {"active": false, "pubkey": "", "endpoints": []}
            ' \
            src-tauri/tauri.conf.json | sponge src-tauri/tauri.conf.json
          '';

          meta = with pkgs.lib; {
            description = "HelixNotes desktop application";
            homepage = "https://helixnotes.com/";
            license = licenses.agpl3Plus;
            platforms = platforms.linux ++ platforms.darwin;
            mainProgram = "helixnotes";
          };
        };

        formatter = pkgs.nixfmt;
      }
    );
}
