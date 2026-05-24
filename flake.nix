{
  description = "gitish — terminal git staging TUI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forEachSystem (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "gitish";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = with pkgs; [ pkg-config cmake git ];
            buildInputs = with pkgs; [ openssl libgit2 zlib ];
            env.OPENSSL_NO_VENDOR = 1;
          };
        });

      apps = forEachSystem (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/gitish";
        };
      });

      devShells = forEachSystem (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              rustc
              cargo
              rust-analyzer
              clippy
              pkg-config
              cmake
            ];
            buildInputs = with pkgs; [
              openssl
              libgit2
              zlib
            ];
            # tells openssl-sys where to find headers + libs
            OPENSSL_NO_VENDOR = 1;
          };
        });
    };
}
