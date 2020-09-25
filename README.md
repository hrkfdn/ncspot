# ncspot

[![Crates.io](https://img.shields.io/crates/v/ncspot.svg)](https://crates.io/crates/ncspot)
[![Gitter](https://badges.gitter.im/ncspot/community.svg)](https://gitter.im/ncspot/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![Build](https://github.com/hrkfdn/ncspot/workflows/Build/badge.svg)](https://github.com/hrkfdn/ncspot/actions?query=workflow%3ABuild)
[![Snap Status](https://build.snapcraft.io/badge/popey/ncspot-snap.svg)](https://build.snapcraft.io/user/popey/ncspot-snap)

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

## Usage

* Install the latest ncspot release using `cargo install ncspot`
  * or build it yourself using `cargo build --release`
  * both approaches require a working [Rust installation](https://www.rust-lang.org/tools/install)
* For debugging, pass a debug log filename, e.g. `ncspot -d debug.log`

## Audio backends

By default ncspot is built using the Rodio backend.  To make it use the
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
* Tracks and playlists can be played using `Return` and queued using `Space`
* `.` will move to the currently playing track in the queue.
* `s` will save, `d` will remove the currently selected track to/from your
  library
* `o` will open a detail view or context menu for the selected item
* `Shift-o` will open a context menu for the currently playing track
* `a` will open the album view for the selected item
* `A` will open the artist view for the selected item
* `Backspace` closes the current view
* `Shift-p` toggles playback of a track
* `Shift-s` stops a track
* `Shift-u` updates the library cache (tracks, artists, albums, playlists)
* `<` and `>` play the previous or next track
* `f` and `b` to seek forward or backward
* `Shift-f` and `Shift-b` to seek forward or backward in steps of 10s
* `-` and `+` decrease or increase the volume
* `r` to toggle repeat mode
* `z` to toggle shuffle playback
* `q` quits ncspot
* `x` copies a sharable URL of the song to the system clipboard
* `Shift-x` copies a sharable URL of the currently selected item to the system clipboard

You can also open a Vim style commandprompt using `:`, the following commands
are supported:

* `quit`: Quit ncspot
* `toggle`: Toggle playback
* `stop`: Stop playback
* `previous`/`next`: Play previous/next track
* `clear`: Clear playlist
* `share [current | selected]`: Copies a sharable URL of either the selected item or the currernt song to the system clipboard
* `newplaylist <name>`: Create new playlist with name `<name>`

The screens can be opened with `queue`, `search`, `playlists` and `log`, whereas
`search` can be supplied with a search term that will be entered after opening
the search view.

## Configuration

Configuration is saved to `~/.config/ncspot/config.toml`. To reload the
configuration during runtime use the `reload` statement in the command prompt
`:reload`.

Possible configuration values are:

* `use_nerdfont`: Turn nerdfont glyphs on/off <true/false>
* `theme`: Set a custom color palette (see below)
* `backend`: Audio backend to use, run `ncspot -h` for a list of devices
* `backend_device`: Audio device string to configure the backend
* `audio_cache`: Enable or disable caching of audio files, on by default
  <true/false>
* `volnorm`: Enable or disable volume normalization, off by default <true/false>
* `volnorm_pregain`: Normalization pregain to apply (if enabled)
* `default_keybindings`: If disabled, the default keybindings are discarded, off
  by default <true/false>
* `notify`: Enable or disable desktop notifications, off by default <true/false>
* `bitrate`: The audio bitrate to use for streaming, can be 96, 160, or 320 (default is 320)


Keybindings can be configured in `[keybindings]` section in `config.toml`, e.g. as such:

```
[keybindings]
"Shift+i" = "seek +10000"
```

See the help screen by pressing `?` for a list of possible commands.

ncspot will respect system proxy settings defined via the `http_proxy`
environment variable.

### Initial state

The initial state can be specified in the configuration.
It allows for example enabling shuffle per default.
Following entries can be added to the configuration file:

```
[saved_state]
volume = 80
repeat = "track"
shuffle = true
```

- `volume` needs to be an integer value between 0 and 100
- `repeat` can be `"track"`, `"playlist"` or any other value which defaults to no
- `shuffle` must be `"true"` or `"false"`

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
```

More examples can be found in pull request
https://github.com/hrkfdn/ncspot/pull/40.
