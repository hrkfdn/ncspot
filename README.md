# ncspot

[![Crates.io](https://img.shields.io/crates/v/ncspot.svg)](https://crates.io/crates/ncspot)
[![Gitter](https://badges.gitter.im/ncspot/community.svg)](https://gitter.im/ncspot/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![Build](https://github.com/hrkfdn/ncspot/workflows/Build/badge.svg)](https://github.com/hrkfdn/ncspot/actions?query=workflow%3ABuild)

[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

[![ncspot](https://snapcraft.io//ncspot/badge.svg)](https://snapcraft.io/ncspot)
[![ncspot](https://snapcraft.io//ncspot/trending.svg?name=0)](https://snapcraft.io/ncspot)

ncurses Spotify client written in Rust using librespot. It is heavily inspired
by ncurses MPD clients, such as ncmpc.  My motivation was to provide a simple
and resource friendly alternative to the official client as well as to support
platforms that currently don't have a Spotify client, such as the *BSDs.

[![Search](/screenshots/screenshot-thumb.png?raw=true)](/screenshots/screenshot.png?raw=true)

## Resource footprint comparison

Measured using `ps_mem` on Linux during playback:

| Client | Private Memory | Shared Memory | Total |
| --- | --- | --- | --- |
| ncspot | 22.1 MiB | 24.1 MiB | 46.2 MiB |
| Spotify | 407.3 MiB | 592.7 MiB | 1000.0 MiB |

## Requirements

### On macOS

ncspot is available via Homebrew: `brew install ncspot`.

### On Linux

* Rust
* Python 3 (needed for building `rust-xcb` dependency)
* `libpulse-dev` (or `portaudio-dev`, if you want to use the PortAudio backend)
* `libncurses-dev` and `libssl-dev`
* `libdbus-1-dev`
* `libxcb` + development headers (for clipboard access)
* A Spotify premium account
* pkg-config

On Debian based systems you need following packages for development headers:
```
sudo apt install libncursesw5-dev libdbus-1-dev libpulse-dev libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

For Fedora, these dependencies are required:
```
dnf install pulseaudio-libs-devel libxcb-devel openssl-devel ncurses-devel dbus-devel
```

#### Building a Debian Package

You can use `cargo-deb` create in order to build a Debian package from source. Install it by:

```
cargo install cargo-deb
```

Then you can build a Dabian package with:

```
cargo deb
```

You can find it under `target/debian`.


### On Windows

Make sure you're using MSVC toolchain

```
cargo install ncspot --no-default-features --features rodio_backend,cursive/pancurses-backend
```

## Usage

* Install the latest ncspot release using `cargo install ncspot`
  * or build it yourself using `cargo build --release`
  * both approaches require a working [Rust installation](https://www.rust-lang.org/tools/install)
* For debugging, you can pass a debug log filename and log stderr to a file, e.g. `RUST_BACKTRACE=full cargo run -- -d debug.log 2> stderr.log`

## Audio backends

By default ncspot is built using the PulseAudio backend.  To make it use the
PortAudio backend (e.g. for *BSD or macOS), you need to recompile ncspot with
the `portaudio_backend` feature:

* `cargo run --no-default-features --features
  portaudio_backend,cursive/pancurses-backend`

### Key Bindings

The keybindings listed below are configured by default. Additionally, if you run
ncspot with MPRIS support, you may be able to use media keys to control playback
depending on your desktop environment settings. Have a look at the
[configuration section](#configuration) if you want to set custom bindings.

* `?` show help screen
* Navigate through the screens using the F-keys:
  * `F1`: Queue
    * `c` clears the entire queue
    * `d` deletes the currently selected track
    * `Ctrl-s` opens a dialog to save the queue to a playlist
  * `F2`: Search
  * `F3`: Library
    * `d` deletes the currently selected playlist
  * `F8`: Album art (if compiled with the `cover` feature)
* Tracks and playlists can be played using `Return` and queued using `Space`
* `.` will play the selected item after the currently playing track
* `p` will move to the currently playing track in the queue
* `s` will save, `d` will remove the currently selected track to/from your
  library
* `o` will open a detail view or context menu for the selected item
  * if the _selected item_ is **not** a track:
    * opens a detail view
  * if the _selected item_ **is** a track:
    * opens a context menu for the _selected item_ presenting 4 options:
      * "Show Artist"
      * "Show Album"
      * "Share"
      * "Add to playlist"
      * "Similar tracks"
* `Shift-o` will open a context menu for the currently playing track
* `a` will open the album view for the selected item
* `A` will open the artist view for the selected item
* `Ctrl-v` will open the context menu for a Spotify link in your clipboard
* `Backspace` closes the current view
* `Shift-p` toggles playback of a track (play/pause)
* `Shift-s` stops a track
* `Shift-u` updates the library cache (tracks, artists, albums, playlists)
* `<` and `>` play the previous or next track
* `f` and `b` to seek forward or backward
* `Shift-f` and `Shift-b` to seek forward or backward in steps of 10s
* `-` and `+` decrease or increase the volume by 1
* `[` and `]` decrease of increase the volume by 5
* `r` to toggle repeat mode
* `z` to toggle shuffle playback
* `q` quits ncspot
* `x` copies a sharable URL of the currently selected item to the system clipboard
* `Shift-x` copies a sharable URL of the currently playing track to the system clipboard

Use `/` to open a Vim-like search bar, you can use `n` and `N` to go for the next/previous
search occurrence, respectivly.

### Commands

You can also open a Vim style commandprompt using `:`, the following commands
are supported:

* `quit`: Quit ncspot
* `logout`: Remove any cached credentials from disk and quit ncspot
* `toggle`: Toggle playback
* `stop`: Stop playback
* `previous`/`next`: Play previous/next track
* `clear`: Clear playlist
* `share [current | selected]`: Copies a sharable URL of either the selected item or the currernt song to the system clipboard
* `newplaylist <name>`: Create new playlist with name `<name>`
* `sort <sort_key> <sort_direction>`: Sort a playlist by `<sort_key>` in direction `<sort_direction>`

  Supported `<sort_key>` are:
    * title
    * album
    * artist
    * duration
    * added

  Supported `<sort_direction>` are:
    * a | asc | ascending
    * d | desc | descending

The screens can be opened with `focus <queue|search|library>`.
The `search` command can be supplied with a search term that will be
entered after opening the search view.

To close the commandprompt at any time, press `esc`.

## Configuration

Configuration is saved to `~/.config/ncspot/config.toml` (or `%AppData%\ncspot\config.toml` on Windows). To reload the
configuration during runtime use the `reload` statement in the command prompt
`:reload`.

Possible configuration values are:

* `use_nerdfont`: Turn nerdfont glyphs on/off <true/false>
* `flip_status_indicators`: By default the statusbar will show a play icon when
   a track is playing and a pause icon when playback is stopped. If this setting
   is enabled, the behavior is reversed. <true/false>
* `theme`: Set a custom color palette (see below)
* `backend`: Audio backend to use, run `ncspot -h` for a list of devices
* `backend_device`: Audio device string to configure the backend
* `audio_cache`: Enable or disable caching of audio files, on by default
  <true/false>
* `audio_cache_size`: Maximum size of audio cache in MiB
* `volnorm`: Enable or disable volume normalization, off by default <true/false>
* `volnorm_pregain`: Normalization pregain to apply (if enabled)
* `default_keybindings`: If disabled, the default keybindings are discarded, off
  by default <true/false>
* `notify`: Enable or disable desktop notifications, off by default <true/false>
* `bitrate`: The audio bitrate to use for streaming, can be 96, 160, or 320 (default is 320)
* `album_column`: Show album column for tracks, on by default <true/false>
* `gapless`: Allows gapless playback <true/false> (default is false)
* `shuffle`: Set default shuffle state <true/false>
* `repeat`: Set default repeat mode <off/track/playlist>


Keybindings can be configured in `[keybindings]` section in `config.toml`, e.g. as such:

```
[keybindings]
"Shift+i" = "seek +10000"
```

See the help screen by pressing `?` for a list of possible commands.

ncspot will respect system proxy settings defined via the `http_proxy`
environment variable.

### Theming

[Theme generator](https://ncspot-theme-generator.vaa.red/) by [@vaarad](https://github.com/vaared).

The color palette can be modified in the configuration. For instance, to have
ncspot match Spotify's official client, you can add the following entries to the
configuration file:

```
[theme]
background = "black"
primary = "light white"
secondary = "light black"
title = "green"
playing = "green"
playing_selected = "light green"
playing_bg = "black"
highlight = "light white"
highlight_bg = "#484848"
error = "light white"
error_bg = "red"
statusbar = "black"
statusbar_progress = "green"
statusbar_bg = "green"
cmdline = "light white"
cmdline_bg = "black"
search_match = "light red"
```

More examples can be found in pull request
https://github.com/hrkfdn/ncspot/pull/40.

### Cover Drawing

When compiled with the `cover` feature, `ncspot` can draw the album art of the current track in a dedicated view (`:focus cover` or `F8` by default) using [Überzug](https://github.com/seebye/ueberzug). For more information on installation and terminal compatibility, consult that repository.

To allow scaling the album art up beyond its resolution (640x640 for Spotify covers), use the config key `cover_max_scale`. This is especially useful for HiDPI displays:

```
cover_max_scale = 2
```

### Authentication

`ncspot` prompts for a Spotify username and password on first launch, uses this to generate an OAuth token, and stores it to disk.

The credentials are stored in `~/.cache/ncspot/librespot/credentials.json` (unless the base path has been changed with the `--basepath` option).

The `:logout` command can be used to programmatically remove cached credentials (see [Commands](#commands) above).
