{ pkgs, lib, config, inputs, ... }:

{
  packages = [ pkgs.cargo-edit ];

  languages.rust = {
    enable = true;
    channel = "stable";
    version = "latest";
  };
}
