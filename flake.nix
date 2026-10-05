{
  description = "Development environment for rankewd";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          fontconfig = pkgs.fontconfig;
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              python3
              uv
              pkg-config
              fontconfig
              fontconfig.dev
            ];

            PKG_CONFIG_PATH = "${fontconfig.dev}/lib/pkgconfig";
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ fontconfig pkgs.freetype ];
          };
        });
    };
}
