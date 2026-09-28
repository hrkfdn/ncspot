{
  pkgs ? import <nixpkgs> { },
}:
pkgs.mkShell {
  nativeBuildInputs = with pkgs.buildPackages; [
    rustup
    pkg-config
    openssl
    pipewire
    alsa-lib
    ncurses
  ];

  shellHook = ''
    export ALSA_PLUGIN_DIR="${pkgs.pipewire}/lib/alsa-lib"
  '';
}
