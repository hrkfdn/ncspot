<div align="center" style="text-align:center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="images/logo_text_dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="images/logo_text_light.svg">
    <img alt="ncspot logo" height="128" src="images/logo_text_light.svg">
  </picture>
  <h3>An ncurses Spotify client written in Rust using librespot</h3>

[![Crates.io](https://img.shields.io/crates/v/ncspot.svg)](https://crates.io/crates/ncspot)
[![Gitter](https://badges.gitter.im/ncspot/community.svg)](https://gitter.im/ncspot/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

  <img alt="ncspot search tab" src="images/screenshot.png">
</div>

ncspot is an ncurses Spotify client written in Rust using librespot. It is heavily inspired by
ncurses MPD clients, such as [ncmpc](https://musicpd.org/clients/ncmpc/). My motivation was to
provide a simple and resource friendly alternative to the official client as well as to support
platforms that currently don't have a Spotify client, such as the \*BSDs.

Since ncspot offers features that are incompatible with free Spotify accounts, it will only work
with a premium account.

## Features
- Support for tracks, albums, playlists, genres, searching...
- Small [resource footprint](doc/resource_footprint.md)
- Support for a lot of platforms
- Vim keybindings out of the box
- IPC socket for remote control
- Automatic authentication using a password manager

## Installation
ncspot is available on macOS (Homebrew), Windows (Scoop) and Linux (native package, Snapcraft and
Flathub). Detailed instructions for each method can be found [here](doc/users.md).

## Configuration
A configuration file can be provided at `$XDG_USER_CONFIG/ncspot/config.toml`. Detailed
configuration information can be found [here](doc/users.md).

## Building
Building ncspot requires a working [Rust installation](https://www.rust-lang.org/tools/install) and
a Python 3 installation. To compile ncspot, run `cargo build`. For detailed instructions on building
ncspot, there is more information [here](doc/developers.md).

## Packaging
Information about provided files and how to generate some of them can be found
[here](doc/package_maintainers.md).
