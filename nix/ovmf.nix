# cSpell:ignore pkgs ovmf stdenv dont uefi fetchurl pname
{ pkgs }:

pkgs.stdenv.mkDerivation {
  pname = "edk2-ovmf";
  version = "nightly";

  src = pkgs.fetchurl {
    url =
      "https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/edk2-ovmf.tar.gz";
    # You'll need to add the sha256 hash after the first build attempt
    # or set it to pkgs.lib.fakeSha256 initially
    sha256 = "sha256-bKiwoRgXfdK0/ACTOsM2FOe7lCcPOBGqm8tzBNiR1w0=";
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
