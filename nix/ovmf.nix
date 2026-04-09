# cSpell:ignore pkgs ovmf stdenv dont uefi fetchurl pname
{ pkgs }:

pkgs.stdenv.mkDerivation {
  pname = "edk2-ovmf";
  version = "nightly-20260409T020240Z";

  src = pkgs.fetchurl {
    url =
      "https://github.com/osdev0/edk2-ovmf-nightly/releases/download/nightly-20260409T020240Z/edk2-ovmf.tar.gz";
    sha256 = "sha256-ht4yQd2UCjaM2N+WVbtVKFbCqPaz9kMSr9Y3abACUBM=";
  };

  dontBuild = true;

  installPhase = ''
    mkdir -p $out
    cp -r * $out/
  '';

  meta = with pkgs.lib; {
    description = "OVMF UEFI firmware for multiple architectures";
    homepage = "https://github.com/osdev0/edk2-ovmf-nightly";
    license = licenses.bsd2;
  };
}
