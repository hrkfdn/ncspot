# Packaging
[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

## Compilation Instructions
ncspot makes use of the standard Cargo build system for everything. To compile a release version,
execute `cargo build --release` in the terminal from the project root. The executable file can be
found at `target/release/ncspot`. For detailed build instructions, have a look at [the developer
documentation](/doc/developers.md).

Additional features can be included by appending them to the build command. A list of all the
available features can be found in the [Cargo.toml](/Cargo.toml) under the `[features]` table. To
activate a feature, include its name like `cargo build --release --features feature1,feature2,...`.
To disable the default features, add `--no-default-features` to the command.

## Other Provided Files
The following is a list of other files that are provided by ncspot. Some of them need to be
generated. Execute `cargo xtask --help` for more information.
- LICENSE
- images/logo.svg (optional)
- misc/ncspot.desktop (for Linux systems)
- misc/ncspot.1 (for Linux systems)
- misc/ncspot.bash (bash completions)
- misc/\_ncspot (zsh completions)
- misc/ncspot.fish (fish completions)
- misc/ncspot.elv (elvish completions)
- misc/\_ncspot.ps1 (powershell completions)

## Building a Debian Package
The [`cargo-deb`](https://github.com/kornelski/cargo-deb#readme) package can be used to build a
Debian package with the following commands. The package will be generated in `target/debian/`.

```sh
cargo install cargo-deb
cargo deb
```

