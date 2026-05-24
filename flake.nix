{
  description = "gitish — terminal git staging TUI";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
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
    };
}
