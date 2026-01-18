# cSpell:ignore pkgs
{ pkgs, lib, config, inputs, ... }:

let ovmf = pkgs.callPackage ./nix/ovmf.nix { };
in {
  claude.code.enable = true;

  languages.rust = {
    enable = true;
    channel = "nightly";
    components =
      [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
    targets = [ "x86_64-unknown-linux-gnu" ];
  };

  # cSpell:disable
  packages = with pkgs; [
    llvmPackages.bintools
    ovmf
    just
    dosfstools
    mtools
    gptfdisk
    qemu
  ];
  # cSpell:enable

  env.RUST_TARGET_PATH = "${config.git.root}/script/targets";
  env.OVMF_DIR = "${ovmf}";
}
