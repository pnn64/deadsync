{
  description = "DeadSync dev environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    self.submodules = true;
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
    }:
    with flake-utils.lib;
    eachSystem allSystems (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;
      in
      {
        packages = rec {
          deadsync = craneLib.buildPackage rec {
            src = ./.;
            strictDeps = true;

            nativeBuildInputs = with pkgs; [
              pkg-config
              makeWrapper
            ];
            buildInputs = with pkgs; [
              alsa-lib
              libGL
              libxcursor
              libxi
              libxkbcommon
              shaderc # NB: this should be in nativeBuildInputs, but otherwise shaderc-sys builds its own copy of shaderc
              udev
              vulkan-loader
              wayland
              xorg.libX11
              xorg.libxcb
            ];

            doCheck = false;

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
            VK_LAYER_PATH = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
            postInstall = ''
              wrapProgram \
                "$out/bin/deadsync" \
                --set LD_LIBRARY_PATH ${LD_LIBRARY_PATH} \
                --set VK_LAYER_PATH ${VK_LAYER_PATH}
            '';
          };
          default = deadsync;
        };

        apps = rec {
          deadsync = flake-utils.lib.mkApp {
            drv = self.packages.${system}.deadsync;
          };
          default = deadsync;
        };

        devShells.default =
          let
            inherit (self.packages.${system}) deadsync;
          in
          craneLib.devShell {
            inputsFrom = [ deadsync ];
            inherit (deadsync) LD_LIBRARY_PATH VK_LAYER_PATH;
          };

        formatter = pkgs.nixfmt;
      }
    );
}
