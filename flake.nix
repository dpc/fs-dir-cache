{
  description = "A CLI tool for CIs and build scripts, making file system based caching easy and correct (locking, eviction, etc.) ";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flakebox = {
      url = "github:rustshop/flakebox?rev=36b349dc4e6802a0a26bafa4baef1f39fbf4e870";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, flakebox }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: {
              # mold wrapper from https://discourse.nixos.org/t/using-mold-as-linker-prevents-libraries-from-being-found/18530/5
              mold =
                let
                  bintools-wrapper = "${nixpkgs}/pkgs/build-support/bintools-wrapper";
                in
                prev.symlinkJoin {
                  name = "mold";
                  paths = [ prev.mold ];
                  nativeBuildInputs = [ prev.makeWrapper ];
                  suffixSalt = prev.lib.replaceStrings [ "-" "." ] [ "_" "_" ] prev.targetPlatform.config;
                  postBuild = ''
                    for bin in ${prev.mold}/bin/*; do
                      rm $out/bin/"$(basename "$bin")"

                      export prog="$bin"
                      substituteAll "${bintools-wrapper}/ld-wrapper.sh" $out/bin/"$(basename "$bin")"
                      chmod +x $out/bin/"$(basename "$bin")"

                      mkdir -p $out/nix-support
                      substituteAll "${bintools-wrapper}/add-flags.sh" $out/nix-support/add-flags.sh
                      substituteAll "${bintools-wrapper}/add-hardening.sh" $out/nix-support/add-hardening.sh
                      substituteAll "${bintools-wrapper}/../wrapper-common/utils.bash" $out/nix-support/utils.bash
                    done
                  '';
                };
            })
          ];

        };
        flakeboxLib = flakebox.lib.${system} { };
        craneLib = flakeboxLib.craneLib;

        src = flakeboxLib.filter.filterSubdirs {
          root = builtins.path {
            name = "htmx-demo";
            path = ./.;
          };
          dirs = [
            "Cargo.toml"
            "Cargo.lock"
            ".cargo"
            "src"
            "static"
          ];
        };

        craneCommonArgs = {
          inherit src;
          nativeBuildInputs = [ pkgs.mold ];
        };
      in
      {
        packages. default = craneLib.buildPackage craneCommonArgs;

        devShells = {
          default = flakeboxLib.mkDevShell {
            packages = [ pkgs.mold ];
          };
        };
      }
    );
}
