# Development

## Prerequisites
- A working [Rust installation](https://www.rust-lang.org/tools/install)
- Python 3 (needed for building `rust-xcb` dependency)

On Linux, you also need:

- `pkgconf` (sometimes called `pkg-config`)
- Development headers for the aforementioned runtime dependencies:
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

## Debugging
For simple debugging, you can pass a debug log filename:

```sh
cargo run -- -d debug.log
```

It can be difficult to debug a TUI application as it might not run well in an IDE terminal or the
terminal could be used by the text editor. It is however possible to run ncspot in its own process
and attach a debugger. On Linux this can be achieved with `gdb` or `lldb`. It is important that
[ptrace](https://www.kernel.org/doc/html/latest/admin-guide/LSM/Yama.html) is disabled for this to
work. To disable it, execute `echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope`. This will allow
any process to inspect the memory of another process. It is automatically re-enabled after a reboot.

If ncspot has crashed you can find the latest backtrace at `$NCSPOT_CACHE_DIRECTORY/backtrace.log`.
The cache directory can be shown by running `ncspot info`.

## Drive harness
For checking the real TUI end to end, ncspot can be driven over its IPC socket. Build with the
`harness` feature (off by default, never in release builds):

```sh
cargo build --features harness
python3 tools/drive.py            # all scenarios except playback
python3 tools/drive.py --play     # also play a track and stop
python3 tools/drive.py show --keep
python3 tools/drive.py sync_time        # time the startup sync; sync_time_cold has no playlist cache
```

The feature adds these commands, also usable from the ncspot command line:

- `harness open <spotify URI or open.spotify.com URL>` pushes the album, artist, show or
  playlist view (a track opens its album, an episode its show).
- `harness open library [tab]` switches to the library screen; `tab` is a tab's config name,
  for example `episodes`.
- `harness drop-session` shuts down the shared playback session, to exercise the reconnect.
- `harness dump <path>` writes a JSON snapshot of the current view: rows, playable rows, paging
  total and busy flag per tab, calls in flight, milliseconds since the last call, the last
  command error, internal API and Web API counters, and rate-limit state. It also reports
  `library_done` (the first library sync finished), `player` (`status`, `position_ms`),
  `session` (`valid`, `reconnects`) and `areas.library` (`internal_ok`, `fallback`, `latched`).
  `internal_ok` counts each page request and each metadata batch, not each sync.

Each scenario runs ncspot in its own tmux server (`nix shell nixpkgs#tmux` is used when tmux is
not installed). `-b` points at a fresh base path holding the cache and config, and
`XDG_RUNTIME_DIR` at a fresh socket directory. The cached login, the library caches and the
user state are copied in, so the startup sync stays incremental. Your own instance, cache and
config are not touched, and the temp directory is deleted unless `--keep`. Each run still costs
some Web API quota for that startup sync. The driver waits for idle before checking, and reports
BLOCKED when a client a scenario needs is rate limited. The driver never prints credentials,
tokens or your username. A Web API token refreshed by a run is written back to the cache when
the cache file was not changed meanwhile. The test instance registers its own MPRIS player and
may show a desktop notification with `--play`.

A view counts as loaded when no tab is busy, no call is in flight and no call has ended for
500 ms. Exit codes: 0 all passed, 1 a scenario failed, 2 setup error, 3 only rate limited.

The library scenarios are `library` (every library tab loaded from the internal API),
`library_delta` (a routine sync costs at most 6 requests), `library_area_off` (with the library
area disabled, the Web API is used and there is no Your Episodes tab) and `reconnect` (playback
resumes after the session is dropped; runs with `--play`). `library_delta` measures the delta
against the sync token from the same run's startup sync. Drive runs never write the library
caches back, so every drive run repeats the one-time startup full fetch (and is slower) until
the real ncspot, built with the internal library sync, has synced and refreshed the cache.

To add a scenario, write a function in `tools/drive.py` that sends commands and checks the dump,
and register it in `SCENARIOS` with the config keys it needs. Headless probes of the same model
paths run with `cargo test probe -- --ignored`. Unit tests of the driver:
`python3 -m unittest tools/drive_test.py`.

## Compiling
Compile and install the latest release with `cargo-install`:

```sh
cargo install ncspot
```

Or clone and build locally:

```sh
git clone https://github.com/hrkfdn/ncspot
cargo build --release
```

**You may need to manually set the audio backend on non-Linux OSes.** See [Audio
Backends](#audio-backends).

## Audio Backends
ncspot uses PulseAudio by default. Support for other backends can be enabled with the following
commands.

PortAudio for BSD's or macOS
```sh
cargo build --no-default-features --features portaudio_backend,pancurses_backend
```

Rodio for Windows
```sh
cargo build --no-default-features --features rodio_backend,pancurses_backend
```

## Other Features
Here are some auxiliary features you may wish to enable:

| Feature           | Default | Description                                                                                |
|-------------------|---------|--------------------------------------------------------------------------------------------|
| `cover`           | off     | Add a screen to show the album art.                                                        |
| `mpris`           | on      | Control `ncspot` via dbus. See [Arch Wiki: MPRIS](https://wiki.archlinux.org/title/MPRIS). |
| `notify`          | on      | Send a notification to show what's playing.                                                |
| `share_clipboard` | on      | Ability to copy the URL of a song/playlist/etc. to system clipboard.                       |

Consult [Cargo.toml](/Cargo.toml) for the full list of supported features.

