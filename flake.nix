{
  description = "The most basic dynamic function row daemon possible";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };
  outputs = { self, nixpkgs }:
    let
      system = "aarch64-linux";
      pkgs = import nixpkgs { inherit system; };
      tiny-dfr = pkgs.rustPlatform.buildRustPackage {
        pname = "tiny-dfr";
        version = "0.4.0-dev";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [ pkg-config makeWrapper ];
        buildInputs = with pkgs; [ libinput libdrm udev ];

        postFixup = ''
          wrapProgram $out/bin/tiny-dfr \
            --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.fontconfig ]}
        '';

        postConfigure = ''
          substituteInPlace etc/systemd/system/tiny-dfr.service \
              --replace-fail /usr/bin $out/bin
        '';

        postInstall = ''
          cp -R etc $out/lib
          cp -R share $out
        '';

        meta = with pkgs.lib; {
          homepage = "https://github.com/WhatAmISupposedToPutHere/tiny-dfr";
          description = "Most basic dynamic function row daemon possible";
          license = [ licenses.asl20 licenses.mit ];
          mainProgram = "tiny-dfr";
          platforms = platforms.linux;
        };
      };
    in
    {
      packages.${system} = {
        inherit tiny-dfr;
        default = tiny-dfr;
      };

      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ tiny-dfr ];
        packages = with pkgs; [
          clippy
          rustfmt
          rust-analyzer
        ];
        RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      };
    };
}
