## Table of Contents

- [Prerequisites](#prerequisites)
- [Debugging](#debugging)
- [Compiling](#compiling)

### Prerequisites

- A working [Rust installation](https://www.rust-lang.org/tools/install)
- Python 3 (needed for building `rust-xcb` dependency)

On Linux, you also need:

- `pkgconf` (or `pkg-config`)
- Development headers for the [aforementioned runtime dependencies](#on-linux)
  - Debian and derivatives:
    ```sh
    sudo apt install libdbus-1-dev libncursesw5-dev libpulse-dev libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
    ```
  - Fedora:
    ```sh
    sudo dnf install dbus-devel libxcb-devel ncurses-devel openssl-devel pulseaudio-libs-devel
    ```
  - Arch and derivatives:
    ```sh
    # headers are included in the base packages
    sudo pacman -S dbus libpulse libxcb ncurses openssl
    ```

### Debugging

For debugging, you can pass a debug log filename:

```sh
cargo run -- -d debug.log
```

If ncspot has crashed you can find the latest backtrace at `~/.cache/ncspot/backtrace.log`.

### Compiling

Compile and install the latest release with `cargo-install`:

```sh
cargo install ncspot
```

Or clone and build locally:

```sh
git clone https://github.com/hrkfdn/ncspot
cargo build --release
```

**You may need to manually set the audio backend on non-Linux OSes.** See
[Audio Backends](#audio-backends).

### Audio Backends

By default `ncspot` is built using the PulseAudio backend. To make it use the
PortAudio backend (e.g. for \*BSD or macOS) or Rodio backend (e.g. for
Windows), you need to compile `ncspot` with the respective features:

```sh
# PortAudio (BSD/macOS)
cargo build --release --no-default-features --features portaudio_backend,pancurses_backend

# Rodio (Windows)
cargo build --release --no-default-features --features rodio_backend,pancurses_backend
```

### Other Features

Here are some auxiliary features you may wish to enable:

| Feature           | Default | Description                                                                                |
|-------------------|---------|--------------------------------------------------------------------------------------------|
| `cover`           | off     | Add a screen to show the album art. See [Cover Drawing](#cover-drawing).                   |
| `mpris`           | on      | Control `ncspot` via dbus. See [Arch Wiki: MPRIS](https://wiki.archlinux.org/title/MPRIS). |
| `notify`          | on      | Send a notification to show what's playing.                                                |
| `share_clipboard` | on      | Ability to copy the URL of a song/playlist/etc. to system clipboard.                       |

Consult [Cargo.toml](Cargo.toml) for the full list of supported features.

