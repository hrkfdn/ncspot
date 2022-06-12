<p align="center" style="text-align:center">
    <img src="images/logo.svg" width="128px" alt="logo"/>
</p>

# ncspot

[![Crates.io](https://img.shields.io/crates/v/ncspot.svg)](https://crates.io/crates/ncspot)
[![Gitter](https://badges.gitter.im/ncspot/community.svg)](https://gitter.im/ncspot/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![Build](https://github.com/hrkfdn/ncspot/workflows/Build/badge.svg)](https://github.com/hrkfdn/ncspot/actions?query=workflow%3ABuild)

[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

[![ncspot](https://snapcraft.io//ncspot/badge.svg)](https://snapcraft.io/ncspot)
[![ncspot](https://snapcraft.io//ncspot/trending.svg?name=0)](https://snapcraft.io/ncspot)

`ncspot` is a ncurses Spotify client written in Rust using `librespot`. It is
heavily inspired by ncurses MPD clients, such as ncmpc. My motivation was to
provide a simple and resource friendly alternative to the official client as
well as to support platforms that currently don't have a Spotify client, such
as the \*BSDs.

[![Search](/images/screenshot-thumb.png?raw=true)](/images/screenshot.png?raw=true)

## Table of Contents

- [ncspot](#ncspot)
  - [Table of Contents](#table-of-contents)
  - [Resource Footprint Comparison](#resource-footprint-comparison)
  - [Installation](#installation)
    - [On macOS](#on-macos)
    - [On Windows](#on-windows)
    - [On Linux](#on-linux)
      - [Building a Debian Package](#building-a-debian-package)
  - [Build](#build)
    - [Audio Backends](#audio-backends)
  - [Key Bindings](#key-bindings)
    - [Navigation](#navigation)
    - [Playback](#playback)
    - [Context Menus](#context-menus)
    - [Sharing](#sharing)
    - [Queue](#queue)
    - [Library](#library)
    - [Vim-Like Search Bar](#vim-like-search-bar)
  - [Vim-Like Commands](#vim-like-commands)
  - [Configuration](#configuration)
    - [Custom Keybindings](#custom-keybindings)
    - [Proxy](#proxy)
    - [Theming](#theming)
    - [Track Formatting](#track-formatting)
  - [Cover Drawing](#cover-drawing)
  - [Authentication](#authentication)

## Resource Footprint Comparison

Measured using `ps_mem` on Linux during playback:

| Client  | Private Memory | Shared Memory | Total      |
|---------|----------------|---------------|------------|
| ncspot  | 22.1 MiB       | 24.1 MiB      | 46.2 MiB   |
| Spotify | 407.3 MiB      | 592.7 MiB     | 1000.0 MiB |

## Installation

### On macOS

`ncspot` is available via [Homebrew](https://brew.sh/):

```zsh
brew install ncspot
```

### On Windows

`ncspot` is available via [Scoop](https://scoop.sh/):

```powershell
scoop install ncspot
```

### On Linux

Requirements:

- Rust
- Python 3 (needed for building `rust-xcb` dependency)
- `libpulse-dev` (or `portaudio-dev`, if you want to use the PortAudio backend)
- `libncurses-dev` and `libssl-dev`
- `libdbus-1-dev`
- `libxcb` + development headers (for clipboard access)
- `pkg-config`
- A Spotify premium account

On Debian based systems you need following packages for development headers:

```bash
sudo apt install libncursesw5-dev libdbus-1-dev libpulse-dev libssl-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

For Fedora, these dependencies are required:

```bash
dnf install pulseaudio-libs-devel libxcb-devel openssl-devel ncurses-devel dbus-devel
```

#### Building a Debian Package

You can use `cargo-deb` create in order to build a Debian package from source.
Install it with:

```bash
cargo install cargo-deb
```

Then you can build a Debian package with:

```bash
cargo deb
```

You can find it under `target/debian`.

## Build

Install the latest `ncspot` release using:

```bash
cargo install ncspot
```

Or build it yourself using:

```bash
cargo build --release

# NB: add these flags on Windows
cargo build --release --no-default-features --features rodio_backend,cursive/pancurses-backend
```

- Both approaches require a working [Rust installation](https://www.rust-lang.org/tools/install).
- For debugging, you can pass a debug log filename and log stderr to a file,
  e.g. :

  ```bash
  RUST_BACKTRACE=full cargo run -- -d debug.log 2> stderr.log
  ```

### Audio Backends

By default `ncspot` is built using the PulseAudio backend. To make it use the
PortAudio backend (e.g. for \*BSD or macOS) or Rodio backend (e.g. for
Windows), you need to recompile `ncspot` with the respective features:

```bash
# PortAudio (BSD/macOS)
cargo run --no-default-features --features portaudio_backend,cursive/pancurses-backend

# Rodio (Windows)
cargo run --no-default-features --features rodio_backend,cursive/pancurses-backend
```

## Key Bindings

The keybindings listed below are configured by default. Additionally, if you
run `ncspot` with MPRIS support, you may be able to use media keys to control
playback depending on your desktop environment settings. Have a look at the
[configuration section](#configuration) if you want to set custom bindings.

### Navigation

| Key               | Command                                                                       |
|:------------------|:------------------------------------------------------------------------------|
| <kbd>?</kbd>      | Show help screen.                                                             |
| <kbd>F1</kbd>     | Queue (See [specific commands](#queue)).                                      |
| <kbd>F2</kbd>     | Search.                                                                       |
| <kbd>F3</kbd>     | Library (See [specific commands](#library)).                                  |
| <kbd>F8</kbd>     | Album Art (if compiled with the `cover` feature).                             |
| <kbd>/</kbd>      | Open a Vim-like search bar (See [specific commands](#vim-like-search-bar)).   |
| <kbd>:</kbd>      | Open a Vim-like command prompt (See [specific commands](#vim-like-commands)). |
| <kbd>Escape</kbd> | Close Vim-like search bar or command prompt.                                  |
| <kbd>Q</kbd>      | Quit `ncspot`.                                                                |

### Playback

| Key                           | Command                                                        |
|:------------------------------|:---------------------------------------------------------------|
| <kbd>Return</kbd>             | Play track or playlist.                                        |
| <kbd>Space</kbd>              | Queue track or playlist.                                       |
| <kbd>.</kbd>                  | Play the selected item after the currently playing track.      |
| <kbd>P</kbd>                  | Move to the currently playing track in the queue.              |
| <kbd>S</kbd>                  | Save the currently playing track to your library.              |
| <kbd>D</kbd>                  | Remove the currently playing track from your library.          |
| <kbd>Shift</kbd>+<kbd>P</kbd> | Toggle playback (i.e. Play/Pause).                             |
| <kbd>Shift</kbd>+<kbd>S</kbd> | Stop playback.                                                 |
| <kbd>Shift</kbd>+<kbd>U</kbd> | Update the library cache (tracks, artists, albums, playlists). |
| <kbd><</kbd>                  | Play the previous track.                                       |
| <kbd>></kbd>                  | Play the next track.                                           |
| <kbd>F</kbd>                  | Seek forward.                                                  |
| <kbd>Shift</kbd>+<kbd>F</kbd> | Seek forward with a 10-second step.                            |
| <kbd>B</kbd>                  | Seek backwards.                                                |
| <kbd>Shift</kbd>+<kbd>B</kbd> | Seek backwards with a 10-second step.                          |
| <kbd>-</kbd>                  | Decrease volume by 1.                                          |
| <kbd>+</kbd>                  | Increase volume by 1.                                          |
| <kbd>[</kbd>                  | Decrease volume by 5.                                          |
| <kbd>]</kbd>                  | Increase volume by 5.                                          |
| <kbd>R</kbd>                  | Toggle _Repeat_ mode.                                          |
| <kbd>Z</kbd>                  | Toggle _Shuffle_ state.                                        |

### Context Menus

| Key                           | Command                                                                |
|:------------------------------|:-----------------------------------------------------------------------|
| <kbd>O</kbd>                  | Open a detail view or context for the **selected item**.               |
| <kbd>Shift</kbd>+<kbd>O</kbd> | Open a context menu for the **currently playing track**.               |
| <kbd>A</kbd>                  | Open the **album view** for the selected item.                         |
| <kbd>Shift</kbd>+<kbd>A</kbd> | Open the **artist view** for the selected item.                        |
| <kbd>M</kbd>                  | Open the **recommendations view** for the **selected item**.           |
| <kbd>Shift</kbd>+<kbd>M</kbd> | Open the **recommendations view** for the **currently playing track**. |
| <kbd>Ctrl</kbd>+<kbd>V</kbd>  | Open the context menu for a Spotify link in your clipboard.            |
| <kbd>Backspace</kbd>          | Close the current view.                                                |

When pressing <kbd>O</kbd>:

- If the _selected item_ is **not** a track, it opens a detail view.
- If the _selected item_ **is** a track, it opens a context menu with:
  - "Show Artist"
  - "Show Album"
  - "Share"
  - "Add to playlist"
  - "Similar tracks"

### Sharing

| Key                           | Command                                                                          |
|:------------------------------|:---------------------------------------------------------------------------------|
| <kbd>X</kbd>                  | Copy a shareable URL of the **currently selected item** to the system clipboard. |
| <kbd>Shift</kbd>+<kbd>X</kbd> | Copy a shareable URL of the **currently playing track** to the system clipboard. |

### Queue

| Key                          | Command                              |
|:-----------------------------|:-------------------------------------|
| <kbd>C</kbd>                 | Clear the entire queue.              |
| <kbd>D</kbd>                 | Delete the currently selected track. |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Delete the currently selected track. |

### Library

| Key          | Command                                 |
|:-------------|:----------------------------------------|
| <kbd>D</kbd> | Delete the currently selected playlist. |

### Vim-Like Search Bar

| Key          | Command                     |
|:-------------|:----------------------------|
| <kbd>n</kbd> | Previous search occurrence. |
| <kbd>N</kbd> | Next search occurrence.     |

## Vim-Like Commands

You can open a Vim-style command prompt using <kbd>:</kbd>, and close it at any
time with <kbd>Escape</kbd>.

The following is an abridged list of the more useful commands. For the full list, see [source code](/src/command.rs).

Note: \<FOO\> - mandatory arg; [BAR] - optional arg

| Command                                                          | Action                                                                                                                                                                                                                                                          |
|------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `help`                                                           | Show current key bindings.                                                                                                                                                                                                                                      |
| `quit`<br/>Aliases: `q`, `x`                                     | Quit `ncspot`.                                                                                                                                                                                                                                                  |
| `logout`                                                         | Remove any cached credentials from disk and quit `ncspot`.                                                                                                                                                                                                      |
| `playpause`<br/>Aliases: `pause`, `toggleplay`, `toggleplayback` | Toggle playback.                                                                                                                                                                                                                                                |
| `stop`                                                           | Stop playback.                                                                                                                                                                                                                                                  |
| `seek` [`+`\|`-`]\<TIME\>                                        | Seek to the specified position, or seek relative to current position by prepending `+`/`-`.<br/>\* TIME is anything accepted by [parse_duration](https://docs.rs/parse_duration/latest/parse_duration/)<br/>\* Default unit is `ms` for backward compatibility. |
| `repeat` [REPEAT_MODE]<br/>Alias: `loop`                         | Set repeat mode. Omit argument to step through the available modes.<br/>\* Valid values for REPEAT_MODE: `list` (aliases: `playlist`, `queue`), `track` (aliases: `once`, `single`), `none` (alias: `off`)                                                      |
| `shuffle` [`on`\|`off`]                                          | Enable or disable shuffle. Omit argument to toggle.                                                                                                                                                                                                             |
| `previous`                                                       | Play the previous track.                                                                                                                                                                                                                                        |
| `next`                                                           | Play the next track.                                                                                                                                                                                                                                            |
| `focus` \<SCREEN\>                                               | Switch to a different view.<br/>\* Valid values for SCREEN: `queue`, `search`, `library`, `cover` (if built with the `cover` feature)                                                                                                                           |
| `search` \<SEARCH\>                                              | Search for a song/artist/album/etc.                                                                                                                                                                                                                             |
| `clear`                                                          | Clear the queue.                                                                                                                                                                                                                                                |
| `share` \<ITEM\>                                                 | Copy a shareable URL of the item to the system clipboard. Requires the `share_clipboard` feature.<br/>\* Valid values for ITEM: `selected`, `current`                                                                                                           |
| `newplaylist` \<NAME\>                                           | Create a new playlist.                                                                                                                                                                                                                                          |
| `sort` \<SORT_KEY\> [SORT_DIRECTION]                             | Sort a playlist.<br/>\* Valid values for SORT_KEY: `title`, `album`, `artist`, `duration`, `added`<br/>\* Valid values for SORT_DIRECTION: `ascending` (default; aliases: `a`, `asc`), `descending` (aliases: `d`, `desc`)                                      |
| `exec` \<CMD\>                                                   | Execute a command in the system shell.<br/>\* Command output is printed to the terminal, so redirection (`2> /dev/null`) may be necessary.                                                                                                                      |
| `noop`                                                           | Do nothing. Useful for disabling default keybindings. See [custom keybindings](#custom-keybindings).                                                                                                                                                            |

## Configuration

Configuration is saved to `~/.config/ncspot/config.toml` (or
`%AppData%\ncspot\config.toml` on Windows). To reload the configuration during
runtime use the command prompt by typing `:reload`.

Possible configuration values are:

| Name                     | Description                                       | Possible values                                                 | Default     |
|--------------------------|---------------------------------------------------|-----------------------------------------------------------------|-------------|
| `command_key`            | Key to open command line                          | Single character                                                | `:`         |
| `initial_screen`         | Screen to show after startup                      | `"library"`, `"search"`, `"queue"`, `"cover"`[^2]               | `"library"` |
| `use_nerdfont`           | Turn nerdfont glyphs on/off                       | `true`, `false`                                                 | `false`     |
| `flip_status_indicators` | Reverse play/pause icon meaning[^1]               | `true`, `false`                                                 | `false`     |
| `backend`                | Audio backend to use                              | String [^3]                                                     |             |
| `backend_device`         | Audio device to configure the backend             | String                                                          |             |
| `audio_cache`            | Enable caching of audio files                     | `true`, `false`                                                 | `true`      |
| `audio_cache_size`       | Maximum size of audio cache in MiB                | Number                                                          |             |
| `volnorm`                | Enable volume normalization                       | `true`, `false`                                                 | `false`     |
| `volnorm_pregain`        | Normalization pregain to apply in dB (if enabled) | Number                                                          | `0.0`       |
| `default_keybindings`    | Enable default keybindings                        | `true`, `false`                                                 | `false`     |
| `notify`                 | Enable desktop notifications                      | `true`, `false`                                                 | `false`     |
| `bitrate`                | Audio bitrate to use for streaming                | `96`, `160`, `320`                                              | `320`       |
| `album_column`           | Show album column for tracks                      | `true`, `false`                                                 | `true`      |
| `gapless`                | Enable gapless playback                           | `true`, `false`                                                 | `true`      |
| `shuffle`                | Set default shuffle state                         | `true`, `false`                                                 | `false`     |
| `repeat`                 | Set default repeat mode                           | `off`, `track`, `playlist`                                      | `off`       |
| `playback_state`         | Set default playback state                        | `"Stopped"`, `"Paused"`, `"Playing"`, `"Default"`               | `"Paused"`  |
| `library_tabs`           | Tabs to show in library screen                    | Array of `tracks`, `albums`, `artists`, `playlists`, `podcasts` | All tabs    |
| `[track_format]`         | Set active fields shown in Library/Queue views    | See [track formatting](#track-formatting)                       |             |
| `[theme]`                | Custom theme                                      | See [custom theme](#theming)                                    |             |
| `[keybindings]`          | Custom keybindings                                | See [custom keybindings](#custom-keybindings)                   |             |

[^1]:
    By default the statusbar will show a play icon when a track is playing and
    a pause icon when playback is stopped. If this setting is enabled, the behavior
    is reversed.

[^2]: If [enabled](#cover-drawing).
[^3]: Run `ncspot -h` for a list of devices.

### Custom Keybindings

Keybindings can be configured in `[keybindings]` section in `config.toml`,
e.g. as such:

```toml
[keybindings]
"Shift+i" = "seek +10s"
```

To disable a default keybinding, set its command to `noop`:

```toml
# Use "Shift+q" to quit instead of the default "q"
[keybindings]
"Shift+q" = "quit"
"q" = "noop"
```

### Proxy

`ncspot` will respect system proxy settings defined via the `http_proxy`
environment variable.

### Theming

[Theme generator](https://ncspot-theme-generator.vaa.red/) by [@vaarad](https://github.com/vaared).

The color palette can be modified in the configuration. For instance, to have
`ncspot` match Spotify's official client, you can add the following entries to
the configuration file:

```toml
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

More examples can be found in [this pull request](https://github.com/hrkfdn/ncspot/pull/40).

### Track Formatting
It's possible to customize which fields are shown in Queue/Library views.
If you don't define `center` for example, the default value will be used.
Available options for tracks:
`%artists`, `%title`, `%album`, `%saved`, `%duration`
Default configuration:

```toml
[track_format]
left = "%artists - %title"
center = "%album"
right = "%saved %duration"
```

<details><summary>Examples: (Click to show/hide)</summary>


Example 1 - Show only album name and track name after it:

```toml
[track_format]
left = "%album"
center = "%title"
right = ""
```

Example 2 - Show track title before artists, and don't show album at all:

```toml
[track_format]
left = "%title - %artists"
center = ""
```

Example 3 - Show everything as default, but hide saved status and track length:

```toml
[track_format]
right = ""
```

Example 4 - Show everything as default, except show title before artists:

```toml
[track_format]
left = "%title - %artists"
```

Example 5 - Show saved status and duration first, followed by track title and artists, with the album last:

```toml
[track_format]
left = "|%saved| %duration | %title - %artists"
center = ""
right = "%album"
```

</details>

## Cover Drawing

When compiled with the `cover` feature, `ncspot` can draw the album art of the
current track in a dedicated view (`:focus cover` or <kbd>F8</kbd> by default)
using [Überzug](https://github.com/seebye/ueberzug). For more information on
installation and terminal compatibility, consult that repository.

To allow scaling the album art up beyond its resolution (640x640 for Spotify
covers), use the config key `cover_max_scale`. This is especially useful for
HiDPI displays:

```toml
cover_max_scale = 2
```

## Authentication

`ncspot` prompts for a Spotify username and password on first launch, uses this
to generate an OAuth token, and stores it to disk.

The credentials are stored in `~/.cache/ncspot/librespot/credentials.json`
(unless the base path has been changed with the `--basepath` option).

The `:logout` command can be used to programmatically remove cached credentials
(see [Vim-Like Commands](#vim-like-commands) above).
