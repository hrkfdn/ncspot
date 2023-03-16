## Table of Contents

- [Packaging Information](#packaging-information)
- [Debian Package](#building-a-debian-package)

[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

## Packaging Information

The following files are provided and should be bundled together with ncspot:
- LICENSE
- images/logo.svg (optional)
- misc/ncspot.desktop (for Linux systems)
- misc/ncspot.1 (for Linux systems)
- misc/ncspot.bash (bash completions)
- misc/\_ncspot (zsh completions)
- misc/ncspot.fish (fish completions)
- misc/ncspot.elv (elvish completions)
- misc/\_ncspot.ps1 (powershell completions)

Some of these files have to be generated. Execute `cargo xtask --help` for more information.

## Building a Debian Package

You can also use `cargo-deb` to build a Debian package

```sh
cargo install cargo-deb
cargo deb
```

You can find the package under `target/debian`.

