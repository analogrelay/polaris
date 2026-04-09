# cSpell:ignore pkgs ovmf stdenv dont uefi fetchurl pname
{ pkgs }:

pkgs.stdenv.mkDerivation {
  pname = "limine";
  version = "11.3.1";

  src = pkgs.fetchFromGitHub {
    owner = "Limine-Bootloader";
    repo = "Limine";
    rev = "v11.3.1-binary";
    hash = "sha256-RrbO6L50IwBQTKmXIjFutz+J6DZXC9LZfiIcwSBKlDM=";
  };

  dontBuild = true;

  installPhase = ''
    mkdir -p $out
    cp -r * $out/
  '';

  meta = with pkgs.lib; {
    description = "Limine Bootloader";
    homepage = "https://github.com/Limine-Bootloader/Limine";
    license = licenses.bsd2;
  };
}
