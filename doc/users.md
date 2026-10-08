# User Documentation

## Installation Instructions
[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

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

or via [winget](https://learn.microsoft.com/en-us/windows/package-manager/winget/):

```powershell
winget install hrkfdn.ncspot
```

### On Linux
<div>
<a href="https://flathub.org/apps/details/io.github.hrkfdn.ncspot"><img width="130" alt="Download on Flathub" src="https://flathub.org/assets/badges/flathub-badge-en.png"/></a>
<a href="https://snapcraft.io/ncspot"><img alt="Download on Snapcraft" src="https://snapcraft.io//ncspot/badge.svg"/></a>
</div>

Your distribution may have packaged `ncspot` in its package repository.
If so, simply install using your distribution's package manager - it
is by far the easiest way. If not, you can build from source instead.
See [Compiling](/doc/developers.md).

In case your package manager does not perform dependency resolution,
here are the runtime dependencies:

- `dbus`, `libncurses`, `libssl`
- `libpulse` (or `portaudio`, if built using the PortAudio backend)
- `libxcb` (if built with the `clipboard` feature)

### On BSD's
Your distribution may have packaged `ncspot` in its package repository.
If so, simply install using your distribution's package manager - it
is by far the easiest way. If not, you can build from source instead.
See [Compiling](/doc/developers.md).

### From [crates.io](https://crates.io/crates/ncspot)

`ncspot` can be installed with `cargo`. The `cargo` documentation recommends against installing
software with it. If another option is available from the ones above, it should be preferred. If no
recent version is available for your OS, you can use the following command to install `ncspot` from
[crates.io](https://crates.io/crates/ncspot).

```zsh
# The --locked option is important and the compilation process might fail without it
cargo install --locked ncspot
```

## Key Bindings
The keybindings listed below are configured by default. Additionally, if you
built `ncspot` with MPRIS support, you may be able to use media keys to control
playback depending on your desktop environment settings. Have a look at the
[configuration section](#configuration) if you want to set custom bindings.

### Count Prefixes
Like Vim, many keys can be prefixed with a number to repeat them that many
times. For example, <kbd>1</kbd><kbd>0</kbd><kbd>j</kbd> moves the selection
down 10 rows and <kbd>3</kbd><kbd>Space</kbd> queues the next 3 items. The
count being typed is shown in the status bar, and is cancelled by
<kbd>Escape</kbd> or by opening the search/command prompt.

Counts apply to the movement keys, <kbd>Space</kbd> and <kbd>.</kbd>, seeking
(<kbd>F</kbd>/<kbd>B</kbd> and their <kbd>Shift</kbd> variants), volume
(<kbd>+</kbd>/<kbd>-</kbd>/<kbd>[</kbd>/<kbd>]</kbd>), track skipping
(<kbd><</kbd>/<kbd>></kbd>) and search occurrences (<kbd>n</kbd>/<kbd>N</kbd>).
One-shot, toggle and menu commands (e.g. <kbd>O</kbd>, <kbd>A</kbd>,
<kbd>M</kbd>, <kbd>Return</kbd>, <kbd>R</kbd>, <kbd>Z</kbd>) ignore the count.

### Navigation
| Key               | Command                                                                       |
|-------------------|-------------------------------------------------------------------------------|
| <kbd>?</kbd>      | Show help screen.                                                             |
| <kbd>F1</kbd>     | Queue (See [specific commands](#queue)).                                      |
| <kbd>F2</kbd>     | Search.                                                                       |
| <kbd>F3</kbd>     | Library (See [specific commands](#library)).                                  |
| <kbd>F8</kbd>     | Album Art (if built with the `cover` feature).                                |
| <kbd>F9</kbd>     | Lyrics (See [specific commands](#lyrics)).                                    |
| <kbd>F10</kbd>    | Jukebox (See [specific commands](#jukebox)).                                  |
| <kbd>/</kbd>      | Open a Vim-like search bar (See [specific commands](#vim-like-search-bar)).   |
| <kbd>:</kbd>      | Open a Vim-like command prompt (See [specific commands](#vim-like-commands)). |
| <kbd>Escape</kbd> | Close Vim-like search bar, command prompt, or Search screen.                  |
| <kbd>Q</kbd>      | Quit `ncspot`.                                                                |
| <kbd>g</kbd>      | Go to the top of the current view (Vim motion).                               |
| <kbd>G</kbd>      | Go to the bottom of the current view (Vim motion).                            |

### Playback
| Key                           | Command                                                        |
|-------------------------------|----------------------------------------------------------------|
| <kbd>Return</kbd>             | Play track or playlist.                                        |
| <kbd>Space</kbd>              | Queue track or playlist.                                       |
| <kbd>.</kbd>                  | Play the selected item after the currently playing track.      |
| <kbd>P</kbd>                  | Move to the currently playing track in the queue.              |
| <kbd>S</kbd>                  | Save the currently playing item to your library.               |
| <kbd>D</kbd>                  | Remove the currently playing item from your library.           |
| <kbd>Shift</kbd>+<kbd>P</kbd> | Toggle playback (i.e. Play/Pause).                             |
| <kbd>Shift</kbd>+<kbd>S</kbd> | Stop playback.                                                 |
| <kbd>Shift</kbd>+<kbd>U</kbd> | Update the library cache (tracks, artists, albums, playlists). |
| <kbd><</kbd>                  | Play the previous track.                                       |
| <kbd>></kbd>                  | Play the next track.                                           |
| <kbd>F</kbd>                  | Seek forward by 1 second.                                      |
| <kbd>Shift</kbd>+<kbd>F</kbd> | Seek forward by 10 seconds.                                    |
| <kbd>B</kbd>                  | Seek backward by 1 second.                                     |
| <kbd>Shift</kbd>+<kbd>B</kbd> | Seek backward by 10 seconds.                                   |
| <kbd>-</kbd>                  | Decrease volume by 1%.                                         |
| <kbd>+</kbd>                  | Increase volume by 1%.                                         |
| <kbd>[</kbd>                  | Decrease volume by 5%.                                         |
| <kbd>]</kbd>                  | Increase volume by 5%.                                         |
| <kbd>R</kbd>                  | Toggle _Repeat_ mode.                                          |
| <kbd>Z</kbd>                  | Toggle _Shuffle_ state.                                        |

### Context Menus
| Key                           | Command                                                                                                   |
|-------------------------------|-----------------------------------------------------------------------------------------------------------|
| <kbd>O</kbd>                  | Open a detail view or context for the **selected item**.                                                  |
| <kbd>Shift</kbd>+<kbd>O</kbd> | Open a context menu for the **currently playing track**.                                                  |
| <kbd>A</kbd>                  | Open the **album view** for the selected item.                                                            |
| <kbd>Shift</kbd>+<kbd>A</kbd> | Open the **artist view** for the selected item.                                                           |
| <kbd>M</kbd>                  | Open the **recommendations view** for the **selected item**.                                              |
| <kbd>Shift</kbd>+<kbd>M</kbd> | Open the **recommendations view** for the **currently playing track**.                                    |
| <kbd>Ctrl</kbd>+<kbd>V</kbd>  | Open the context menu for a Spotify link in your clipboard (if built with the `share_clipboard` feature). |
| <kbd>Backspace</kbd>          | Close the current view.                                                                                   |

When pressing <kbd>O</kbd>:

- If the _selected item_ is **not** a track, it opens a detail view.
- If the _selected item_ **is** a track, it opens a context menu with:
  - "Artist(s)" (let's you show or (un)follow a track's artist(s))
  - "Show Album"
  - "Share" (if built with the `share_clipboard` feature)
  - "Add to playlist"
  - "Similar tracks"

### Sharing
(if built with the `share_clipboard` feature)

| Key                           | Command                                                                  |
|-------------------------------|--------------------------------------------------------------------------|
| <kbd>X</kbd>                  | Copy the URL to the **currently selected item** to the system clipboard. |
| <kbd>Shift</kbd>+<kbd>X</kbd> | Copy the URL to the **currently playing track** to the system clipboard. |

### Queue
| Key                          | Command                              |
|------------------------------|--------------------------------------|
| <kbd>C</kbd>                 | Clear the entire queue.              |
| <kbd>D</kbd>                 | Delete the currently selected track. |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> | Save the current queue.              |

### Library
| Key          | Command                                 |
|--------------|-----------------------------------------|
| <kbd>D</kbd> | Delete the currently selected playlist. |

### Vim-Like Search Bar
| Key          | Command                     |
|--------------|-----------------------------|
| <kbd>n</kbd> | Previous search occurrence. |
| <kbd>N</kbd> | Next search occurrence.     |

### Lyrics
These keys are active only while the lyrics screen is focused (<kbd>F9</kbd> or `:focus lyrics`).
You can also **left-click** any line to jump playback there. The `[` and `]` keys override
their global volume-change meaning on this screen.

| Key                           | Command                                                                 |
|-------------------------------|-------------------------------------------------------------------------|
| <kbd>j</kbd> / <kbd>↓</kbd>  | Move the selection cursor down one line.                               |
| <kbd>k</kbd> / <kbd>↑</kbd>  | Move the selection cursor up one line.                                 |
| <kbd>Enter</kbd>              | Seek playback to the selected line (synced lyrics only).               |
| <kbd>Esc</kbd>                | Clear the selection and resume auto-follow.                            |
| <kbd>[</kbd>                  | Shift sync offset back by 500 ms.                                       |
| <kbd>]</kbd>                  | Shift sync offset forward by 500 ms.                                    |
| <kbd>y</kbd>                  | Copy the current line to the system clipboard.                          |
| <kbd>Y</kbd>                  | Copy all lyrics to the system clipboard.                                |

The underlying commands (`lyricsscrollup`, `lyricsscrolldown`, `lyricsseek`, `lyricsoffset`,
`lyricsprovider`, `lyricsrefetch`, `lyricscopyline`, `lyricscopyall`) can be bound to any key
via `[keybindings]`. Each interaction can be individually disabled via the `allow_*` fields in
`[lyrics]` (see [Lyrics settings](#lyrics-settings)).

### Jukebox
The Eternal Jukebox plays the current track endlessly by seeking between similar-sounding
beats. Toggle it from anywhere with `:jukeboxtoggle`, and open the visualizer with
<kbd>F10</kbd> or `:focus jukebox`. These keys are active while the jukebox screen is focused.

| Key                          | Command                                                       |
|------------------------------|---------------------------------------------------------------|
| <kbd>h</kbd> / <kbd>←</kbd> | Move the beat cursor left.                                     |
| <kbd>l</kbd> / <kbd>→</kbd> | Move the beat cursor right.                                    |
| <kbd>Enter</kbd>             | Seek to the selected beat.                                     |
| <kbd>v</kbd>                 | Cycle the layout (linear track → radial ring → split panel).  |
| <kbd>b</kbd>                 | Toggle "bounce" (repeat the current section).                 |
| <kbd>i</kbd>                 | Toggle endless mode.                                           |
| <kbd>g</kbd>                 | Toggle the graphics backend (image vs ASCII), if compiled in. |
| <kbd>s</kbd>                 | Open the per-song jukebox settings menu.                      |

The underlying commands (`jukeboxtoggle`, `jukeboxview`, `jukeboxbounce`, `jukeboxseek`,
`jukeboxsettings`, `jukeboxgraphics`, `jukeboxsongsettings`) can be bound to any key via `[keybindings]`. Analysis is
fetched from Spotify's internal client endpoint, with a configurable eternalbox.dev fallback;
tracks without analysis (local files, podcasts) cannot be jukeboxed. See
[Jukebox settings](#jukebox-settings).

When built with the `jukebox-graphics` feature, the visualizer can render as a real image
(smooth circle, anti-aliased branches) on terminals that support the kitty graphics protocol,
iTerm2, or sixel. Toggle it at runtime with <kbd>g</kbd> or `:jukeboxgraphics`; it falls back
to the ASCII renderer when off or unsupported. Set the startup default with `[jukebox]
graphics` (default `true` when the feature is compiled). On kitty the image is transmitted as
PNG and replaced in place with a transparent background, so it blends with the terminal and
stays cheap even at high `graphics_max_px`; on sixel/iTerm2 it uses your theme's background
colour (falling back to black for the default `TerminalDefault`).

Press <kbd>s</kbd> or run `:jukeboxsongsettings` to open the per-song settings menu, which
lets you save dial overrides for the current track (as a full snapshot or a diff from the
global settings). In the Split view the dial read-out marks per-song overrides with `▸` and
global-but-changed dials with `·`.

### Vim-Like Commands
You can open a Vim-style command prompt using <kbd>:</kbd>,
trigger command completion by pressing <kbd>Tab</kbd>
and close it at any time with <kbd>Escape</kbd>.

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
| `move` \<DIRECTION\> \<STEP_SIZE\>                               | Scroll the current view `up`/`down`/`left`/`right` with integer step sizes, or `pageup`/`pagedown`/`pageleft`/`pageright` with float step sizes.                                                                                                                |
| `repeat` [REPEAT_MODE]<br/>Alias: `loop`                         | Set repeat mode. Omit argument to step through the available modes.<br/>\* Valid values for REPEAT_MODE: `list` (aliases: `playlist`, `queue`), `track` (aliases: `once`, `single`), `none` (alias: `off`)                                                      |
| `shuffle` [`on`\|`off`]                                          | Enable or disable shuffle. Omit argument to toggle.                                                                                                                                                                                                             |
| `previous`                                                       | Play the previous track.                                                                                                                                                                                                                                        |
| `next`                                                           | Play the next track.                                                                                                                                                                                                                                            |
| `focus` \<SCREEN\>                                               | Switch to a different view.<br/>\* Valid values for SCREEN: `queue`, `search`, `library`, `cover` (if built with the `cover` feature), `lyrics`                                                                                                                 |
| `search` \<SEARCH\>                                              | Search for a song/artist/album/etc.                                                                                                                                                                                                                             |
| `clear`                                                          | Clear the queue.                                                                                                                                                                                                                                                |
| `share` \<ITEM\>                                                 | Copy a shareable URL of the item to the system clipboard. Requires the `share_clipboard` feature.<br/>\* Valid values for ITEM: `selected`, `current`                                                                                                           |
| `newplaylist` \<NAME\>                                           | Create a new playlist.                                                                                                                                                                                                                                          |
| `sort` \<SORT_KEY\> [SORT_DIRECTION]                             | Sort a playlist.<br/>\* Valid values for SORT_KEY: `title`, `album`, `artist`, `duration`, `added`<br/>\* Valid values for SORT_DIRECTION: `ascending` (default; aliases: `a`, `asc`), `descending` (aliases: `d`, `desc`)                                      |
| `exec` \<CMD\>                                                   | Execute a command in the system shell.<br/>\* Command output is printed to the terminal, so redirection (`2> /dev/null`) may be necessary.                                                                                                                      |
| `noop`                                                           | Do nothing. Useful for disabling default keybindings. See [custom keybindings](#custom-keybindings).                                                                                                                                                            |
| `reload`                                                         | Reload the configuration from disk. See [Configuration](#configuration).                                                                                                                                                                                        |
| `reconnect`                                                      | Reconnect to Spotify (useful when session has expired or connection was lost)                                                                                                                                                                                   |
| `add [current]`                                                  | Add selected track to playlist, if `current` is passed the currently playing track will be added                                                                                                                                                                |
| `save [current]`                                                 | Save selected item, if `current` is passed the currently playing item will be saved                                                                                                                                                                             |

## Remote control (IPC)
Apart from MPRIS, ncspot will also create a domain socket on UNIX platforms (Linux, macOS, *BSD).
The socket will be created in the platform's runtime directory. Run `ncspot info` to show the
location of this directory on your platform. Applications or scripts can connect to this socket to
send commands or be notified of the currently playing track, i.e. with `netcat`:

```
% nc -U $NCSPOT_CACHE_DIRECTORY/ncspot.sock
play
{"mode":{"Playing":{"secs_since_epoch":1672249086,"nanos_since_epoch":547517730}},"playable":{"type":"Track","id":"2wcrQZ7ZJolYEfIaPP9yL4","uri":"spotify:track:2wcrQZ7ZJolYEfIaPP9yL4","title":"Hit Me Where It Hurts","track_number":4,"disc_number":1,"duration":184132,"artists":["Caroline Polachek"],"artist_ids":["4Ge8xMJNwt6EEXOzVXju9a"],"album":"Pang","album_id":"4ClyeVlAKJJViIyfVW0yQD","album_artists":["Caroline Polachek"],"cover_url":"https://i.scdn.co/image/ab67616d0000b2737d983e7bf67c2806218c2759","url":"https://open.spotify.com/track/2wcrQZ7ZJolYEfIaPP9yL4","added_at":"2022-12-19T22:41:05Z","list_index":0}}
playpause
{"mode":{"Paused":{"secs":25,"nanos":575000000}},"playable":{"type":"Track","id":"2wcrQZ7ZJolYEfIaPP9yL4","uri":"spotify:track:2wcrQZ7ZJolYEfIaPP9yL4","title":"Hit Me Where It Hurts","track_number":4,"disc_number":1,"duration":184132,"artists":["Caroline Polachek"],"artist_ids":["4Ge8xMJNwt6EEXOzVXju9a"],"album":"Pang","album_id":"4ClyeVlAKJJViIyfVW0yQD","album_artists":["Caroline Polachek"],"cover_url":"https://i.scdn.co/image/ab67616d0000b2737d983e7bf67c2806218c2759","url":"https://open.spotify.com/track/2wcrQZ7ZJolYEfIaPP9yL4","added_at":"2022-12-19T22:41:05Z","list_index":0}}
```

Each time the playback status changes (i.e. after sending the `play`/`playpause`
command or simply by playing the queue), the current status will be published as
a JSON structure.

Possible use cases for this could be:
- Controlling a detached ncspot session (in `tmux` for example)
- Displaying the currently playing track in your favorite application/status bar (see below)
- Setting up routines, i.e. to play specific songs/playlists when ncspot starts

### Extracting info on currently playing song
Using `netcat` and the domain socket, you can query the currently playing track
and other relevant information. Note that not all `netcat` versions are suitable,
as they typically tend to keep the connection to the socket open. OpenBSD's
`netcat` offers a work-around: by using the `-W` flag, it will close after a
specific number of packets have been received.

```
% nc -W 1 -U $NCSPOT_CACHE_DIRECTORY/ncspot.sock
{"mode":{"Playing":{"secs_since_epoch":1675188934,"nanos_since_epoch":50913345}},"playable":{"type":"Track","id":"5Cp6a1h2VnuOtsh1Nqxfv6","uri":"spotify:track:5Cp6a1h2VnuOtsh1Nqxfv6","title":"New Track","track_number":1,"disc_number":1,"duration":498358,"artists":["Francis Bebey"],"artist_ids":["0mdmrbu5UZ32uRcRp2z6mr"],"album":"African Electronic Music (1975-1982)","album_id":"7w99Aae1tYSTSb1OiDnxYY","album_artists":["Francis Bebey"],"cover_url":"https://i.scdn.co/image/ab67616d0000b2736ab57cedf27177fae1eaed87","url":"https://open.spotify.com/track/5Cp6a1h2VnuOtsh1Nqxfv6","added_at":"2020-12-22T09:57:17Z","list_index":0}}
```

This results in a single output in `JSON` format, which can e.g. be parsed using [jq](https://stedolan.github.io/jq/).
For example, you can get the currently playing artist and title in your
terminal as follows:

```
% nc -W 1 -U $NCSPOT_CACHE_DIRECTORY/ncspot.sock | jq '.playable.title'
"PUMPIN' JUMPIN'"

% nc -W 1 -U $NCSPOT_CACHE_DIRECTORY/ncspot.sock | jq '.playable.artists[0]'
"Hideki Naganuma"
```

## Configuration
Configuration is saved to the `config.toml` file in the platform's standard configuration directory.
Run `ncspot info` to show the location of this directory on your platform. To reload the
configuration during runtime use the `reload` command.

Possible configuration values are:

| Name                            | Description                                                    | Possible values                                                                       | Default             |
|---------------------------------|----------------------------------------------------------------|---------------------------------------------------------------------------------------|---------------------|
| `command_key`                   | Key to open command line                                       | Single character                                                                      | `:`                 |
| `initial_screen`                | Screen to show after startup                                   | `"library"`, `"search"`, `"queue"`, `"cover"`<sup>[1]</sup>                           | `"library"`         |
| `use_nerdfont`                  | Turn nerdfont glyphs on/off                                    | `true`, `false`                                                                       | `false`             |
| `flip_status_indicators`        | Reverse play/pause icon meaning<sup>[2]</sup>                  | `true`, `false`                                                                       | `false`             |
| `backend`                       | Audio backend to use                                           | String<sup>[3]</sup>                                                                  |                     |
| `backend_device`                | Audio device to configure the backend                          | String                                                                                |                     |
| `audio_cache`                   | Enable caching of audio files                                  | `true`, `false`                                                                       | `true`              |
| `audio_cache_size`              | Maximum size of audio cache in MiB                             | Number                                                                                |                     |
| `volnorm`                       | Enable volume normalization                                    | `true`, `false`                                                                       | `false`             |
| `volnorm_pregain`               | Normalization pregain to apply in dB (if enabled)              | Number                                                                                | `0.0`               |
| `default_keybindings`           | Enable default keybindings                                     | `true`, `false`                                                                       | `false`             |
| `notify`<sup>[4]</sup>          | Enable desktop notifications                                   | `true`, `false`                                                                       | `false`             |
| `notify_rate_limit`<sup>[4]</sup> | Notify when Spotify rate limits ncspot (HTTP 429)            | `true`, `false`                                                                       | `true`              |
| `bitrate`                       | Audio bitrate to use for streaming                             | `96`, `160`, `320`                                                                    | `320`               |
| `gapless`                       | Enable gapless playback                                        | `true`, `false`                                                                       | `true`              |
| `shuffle`                       | Set default shuffle state                                      | `true`, `false`                                                                       | `false`             |
| `repeat`                        | Set default repeat mode                                        | `"off"`, `"track"`, `"playlist"`                                                      | `"off"`             |
| `playback_state`                | Set default playback state                                     | `"Stopped"`, `"Paused"`, `"Playing"`, `"Default"`                                     | `"Paused"`          |
| `library_tabs`                  | Tabs to show in library screen                                 | Array of `"tracks"`, `"albums"`, `"artists"`, `"playlists"`, `"podcasts"`, `"browse"` | All tabs            |
| `cover_max_scale`<sup>[1]</sup> | Set maximum scaling ratio for cover art                        | Number                                                                                | No limit            |
| `hide_display_names`            | Hides spotify usernames in the library header and on playlists | `true`, `false`                                                                       | `false`             |
| `statusbar_format`              | Formatting for tracks in the statusbar                         | See [track_formatting](#track-formatting)                                             | `%artists - %track` |
| `[track_format]`                | Set active fields shown in Library/Queue views                 | See [track formatting](#track-formatting)                                             |                     |
| `[notification_format]`         | Set the text displayed in notifications<sup>[4]</sup>          | See [notification formatting](#notification-formatting)                               |                     |
| `[theme]`                       | Custom theme                                                   | See [custom theme](#theming)                                                          |                     |
| `[keybindings]`                 | Custom keybindings                                             | See [custom keybindings](#custom-keybindings)                                         |                     |
| `[lyrics]`                      | Lyrics feature settings                                        | See [lyrics settings](#lyrics-settings)                                               |                     |
| `[jukebox]`                     | Eternal Jukebox settings                                       | See [jukebox settings](#jukebox-settings)                                             |                     |
| `ap_port`                       | Set ap-port for librespot (for restrictive firewalls)          | `80`, `443`, `4070`                                                                   |                     |
| `client_id`                     | Personal Spotify Web API client ID                             | 32 hex characters, see [personal client ID](#personal-web-api-client-id)              |                     |

1. If built with the `cover` feature.
2. By default the statusbar will show a play icon when a track is playing and
   a pause icon when playback is stopped. If this setting is enabled, the behavior
   is reversed.
3. Run `ncspot -h` for a list of devices.
4. If built with the `notify` feature.

### Custom Keybindings
Keybindings can be configured in `[keybindings]` section in `config.toml`.

Each key-value pair specifies one keybinding, where the key is a string in the
format of:

```
[MODIFIER+]<CHAR|NAMED_KEY>
where:
  MODIFIER: Shift|Alt|Ctrl
  CHAR: Any printable character
  NAMED_KEY: Enter|Space|Tab|Backspace|Esc|Left|Right|Up|Down
    |Ins|Del|Home|End|PageUp|PageDown|PauseBreak|NumpadCenter
    |F0|F1|F2|F3|F4|F5|F6|F7|F8|F9|F10|F11|F12
```

For implementation see [commands::CommandManager::parse_key](/src/commands.rs).

Its value is a string that can be parsed as a command. See
[Vim-Like Commands](#vim-like-commands).

<details>
  <summary>Examples: (Click to show/hide)</summary>

```toml
[keybindings]
# Bind "Shift+i" to "Seek forward 10 seconds"
"Shift+i" = "seek +10s"
```

To disable a default keybinding, set its command to `noop`:

```toml
# Use "Shift+q" to quit instead of the default "q"
[keybindings]
"Shift+q" = "quit"
"q" = "noop"
```

</details>

### Personal Web API client ID

By default, ncspot's Web API requests (search, playlists, library, metadata)
use a client ID shared by every ncspot user. Spotify limits requests per
client ID, so heavy use by everyone together can get ncspot rate limited
(HTTP 429): playlists load slowly or not at all. While that happens, the
status bar shows `[rate limited: shared 42s]` and, if built with the `notify`
feature, a desktop notification appears (disable it with
`notify_rate_limit = false`).

You can register your own Spotify app and give ncspot its client ID. Bulk
traffic (playlist sync, library, track and album metadata) then uses your
own quota.

1. Go to <https://developer.spotify.com/dashboard> and create an app. Your
   account needs Spotify Premium.
2. Add the redirect URI `http://127.0.0.1/ncspot_login`, with no port.
   Spotify allows any port on loopback addresses when the registered URI
   has none, and ncspot picks a free port at runtime.
3. Select "Web API" as the API used, save, and copy the client ID.
4. Give ncspot the client ID in one of these ways (highest precedence first):
   - command line: `ncspot --client-id <ID>`
   - environment variable: `NCSPOT_CLIENT_ID=<ID>`
   - configuration file: `client_id = "<ID>"`

   An empty value is skipped. An invalid value (not 32 hex characters) stops
   ncspot with an error naming where it came from. The client ID is read at
   startup; restart ncspot after changing it.

On the next start, ncspot asks for one more browser authorization for the
new app. Its token is cached in `rspotify_token-<ID>.json` in the cache
directory (see `ncspot info`). At every start ncspot checks that token; if
Spotify rejects it (for example after you removed the app's access), the
browser authorization runs again before the interface opens. If your
Spotify account is not the app owner, add it under User Management in the
app's dashboard, otherwise ncspot falls back to the shared client ID.

ncspot never asks for a login while the interface is running. If the
personal token stops working mid-session, ncspot switches to the shared
client ID, shows `Personal client ID needs a new login` and, with the
`notify` feature, a desktop notification. Restart ncspot to log in again.

Only the client ID is needed: ncspot authorizes as a public PKCE client with
a loopback redirect, so no client secret is ever asked for or stored. Do not
put your app's client secret in the configuration.

New apps run in Spotify's Development Mode, which can't use some endpoints.
ncspot keeps using the shared client ID for those, so these features keep
working: search, recommendations, an artist's top tracks and related
artists, browse categories, and Spotify-owned playlists such as Discover
Weekly. Development Mode apps are also limited to 5 users, and the app owner
needs Premium.

### Proxy

All of ncspot's network traffic (the Spotify access point, the Spotify APIs,
the OAuth login flow and cover art downloads) can be routed through a proxy
configured in the configuration file:

```toml
# HTTP CONNECT proxy (optionally with credentials)
proxy = "http://user:password@foo.bar:4444"

# SOCKS5 proxy with remote DNS resolution (the proxy resolves hostnames)
proxy = "socks5h://foo.bar:1080"
proxy = "socks5h://user:password@foo.bar:1080"
```

`socks5://` is accepted as an alias for `socks5h://`; in both cases the
hostname is resolved by the proxy, which is usually what you want when local
DNS is unusable.

Alternatively, the legacy behavior is still supported: without a `proxy`
setting, ncspot honours the `http_proxy` environment variable (HTTP CONNECT
proxies only).

```sh
# In sh-like shells
http_proxy="http://foo.bar:4444" ncspot
```

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
statusbar_warning = "light red"
cmdline = "light white"
cmdline_bg = "black"
search_match = "light red"
lyrics_highlight = "green"
lyrics_secondary = "light black"
```

More examples can be found in [this pull request](https://github.com/hrkfdn/ncspot/pull/40).

### Track Formatting
It's possible to customize how tracks are shown in Queue/Library views and the
statusbar, whereas `statusbar_format` will hold the statusbar formatting and
`[track_format]` the formatting for tracks in list views.
If you don't define `center` for example, the default value will be used.
Available options for tracks: `%artists`, `%artist`, `%title`, `%album`, `%saved`,
`%duration`.
`%artists` will show all contributing artists, while `%artist` only shows the first listed artist.

Default configuration:

```toml
statusbar_format = "%artists - %title"

[track_format]
left = "%artists - %title"
center = "%album"
right = "%saved %duration"
```

<details>
  <summary>Examples: (Click to show/hide)</summary>

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

### Notification Formatting
`ncspot` also supports customizing the way notifications are displayed
(which appear when compiled with the `notify` feature and `notify = true`).
The title and body of the notification can be set, with `title` and `body`, or the default will be used.
The formatting options are the same as those for [track formatting](#track-formatting) (`%artists`, `%title`, etc)

Default configuration:

```toml
[notification_format]
title = "%title"
body = "%artists"
```

### Cover Drawing
When compiled with the `cover` feature, `ncspot` can draw the album art of the
current track in a dedicated view (`:focus cover` or <kbd>F8</kbd> by default)
using terminal image protocols supported by your terminal, such as Kitty,
iTerm2, or Sixel. If no supported graphics protocol is detected, `ncspot`
falls back to colored terminal blocks.

By default, cover art scales to fit the available terminal view. To limit
scaling beyond the image's native resolution, use the config key
`cover_max_scale`:

```toml
cover_max_scale = 2
```

### Lyrics Settings
The lyrics feature fetches and displays synced and plain-text lyrics for the currently playing
track. Open the lyrics screen with <kbd>F9</kbd> or `:focus lyrics`.

```toml
[lyrics]
# Set to false to disable the lyrics feature entirely.
enabled = true

# Provider search order. Remove an entry to skip that provider entirely.
# Valid values: "lrclib", "spotify", "netease", "musixmatch"
providers = ["lrclib", "spotify", "netease", "musixmatch"]

# User-supplied Musixmatch token. Musixmatch is skipped unless a non-empty token is set here.
# musixmatch_token = "your_token_here"

# Show provider-supplied translation lines beneath each original line (when available).
show_translation = true

# Show provider-supplied romaji lines beneath each original line (when available).
show_romaji = true

# Perform client-side romanization for Chinese (pinyin) and Korean (Hangul) when
# the provider supplies no romaji of its own.
romanize_client_side = true

# Remember the sync offset across track changes and sessions.
persist_offset = true

# Allow j/k scrolling (manual override of the auto-follow behavior).
allow_scroll = true

# Allow [ / ] to adjust the sync offset.
allow_offset = true

# Allow cycling to the next provider via the lyricsprovider command.
allow_provider_switch = true

# Allow copying lyrics to the clipboard (y / Y keys, or lyricscopyline / lyricscopyall commands).
allow_copy = true

# Allow seeking by clicking a line or pressing Enter on the selected line
# (synced lyrics only; via the lyricsseek command).
allow_seek = true
```

The `musixmatch_token` field is intentionally optional and opt-in: Musixmatch will not be
contacted unless you supply a token. Tokens can be obtained from the Musixmatch developer portal.

To rebind any in-page action, use `[keybindings]` with the corresponding command name. For
example, to bind <kbd>p</kbd> to cycle providers and <kbd>r</kbd> to re-fetch:

```toml
[keybindings]
"p" = "lyricsprovider"
"r" = "lyricsrefetch"
```

### Jukebox Settings
The Eternal Jukebox finds pathways through similar beats of a song and plays a never-ending,
ever-changing version of it. Open the jukebox screen with <kbd>F10</kbd> or `:focus jukebox`,
and toggle endless mode with `:jukeboxtoggle`. All settings are optional; the defaults match
the original. They can also be tuned **live** via the in-app settings modal
(`:jukeboxsettings`), which now covers the anti-loop options too. Changes made in the modal
are **persisted** (to the runtime state file, not `config.toml`) and take precedence over
`config.toml` on the next launch; the modal's **Reset** button clears the persisted values
and restores your `config.toml`. The endless-mode and graphics on/off toggles are not
persisted, so the jukebox always starts disabled.

```toml
[jukebox]
# Set to true to start with endless mode already on.
enabled = false

# Maximum allowed "distance" between two beats for a branch. Higher = more branches.
branch_similarity_threshold = 30

# Ignore the fixed threshold and pick one dynamically to hit a target branch count.
dynamic_threshold = false

# Branch probability ramps from min to max, rising by the ramp value each non-branching beat.
min_branch_probability = 0.18
max_branch_probability = 0.5
branch_probability_ramp = 0.018

# Only add branches that go backward in the song.
only_backward_branches = false

# Only add long branches (covering at least a fifth of the song).
only_long_branches = false

# Remove consecutive branches of the same length.
remove_sequential_branches = false

# Add the best long backward branch at the last branch point (helps avoid short loops).
add_last_branch = true

# Always branch at the last possible point so the song keeps looping.
always_follow_last_branch = true

# Stop branching after this many seconds of jukebox playback (0 = never stop).
max_play_time_secs = 0

# Anti-loop: skip a branch once it has been taken too many times, to break short loops.
break_loops = false
# Take a branch this many times before skipping it.
loop_threshold = 4
# What counts as "the same branch": "edge" (source->dest), "destination", or "distance".
loop_identity = "edge"
# How repeats are counted: "consecutive" (resets when a different branch is taken) or
# "cumulative" (lifetime total per branch).
loop_count_mode = "consecutive"
# On limit: "different_else_continue" (take another branch, else continue), "continue"
# (always continue past it), or "different_only" (only ever substitute another branch).
loop_skip_action = "different_else_continue"
# Whether the forced eternal "last branch" can be skipped too (true can let the song end).
break_last_branch = false
# After a skip: "reset" the counter (loop can re-form) or "retire" the branch for the track
# (cumulative only).
loop_counter = "reset"

# Order of analysis sources to try. Valid values: "spclient", "eternalbox".
analysis_sources = ["spclient", "eternalbox"]

# Base URL of the eternalbox fallback instance.
eternalbox_url = "https://eternalbox.dev"

# Visualizer: draw the full branch graph ("web"). false = only the active branch.
show_all_branches = true

# Cap how many branches are drawn (0 = unlimited). Lower this to reduce clutter on
# songs with many branches.
max_branches_drawn = 0

# Which layouts draw the full web; others show only the active branch.
# Valid values: "linear", "radial", "split". (The radial layout reads cleanest when dense.)
branch_layouts = ["linear", "radial", "split"]

# Default graphics backend on/off at startup (only when built with the jukebox-graphics
# feature). Renders the visualizer as a real image on kitty/iTerm2/sixel terminals.
# Toggle live with `g` / :jukeboxgraphics.
graphics = true

# Cap the rendered graphics image's longest edge (pixels). When unset, kitty renders at
# native cell-box resolution (full fidelity, since PNG transmission is cheap) while sixel/
# iTerm2 caps to 1280. Set it to also cap kitty (lower if input ever feels laggy), or to
# raise the sixel/iTerm2 cap.
# graphics_max_px = 1280

# When saving per-song settings, store the full dial snapshot (true) or only dials that
# differ from global settings (false). Toggled in the per-song settings menu.
# override_per_song = false
```

With `break_loops` on, the jukebox tracks how often each branch is taken and, once a branch
hits `loop_threshold`, skips it to break short loops. A future `loop_count_mode = "cycle"`
(not yet implemented) would detect whole repeating beat-cycles rather than single branches.

Analysis comes from Spotify's internal client endpoint first; if that has no data for a track,
the configurable eternalbox instance is tried (it only serves songs it has already analyzed).
Tracks without analysis, such as local files and podcasts, cannot be jukeboxed.

## Authentication
`ncspot` uses OAuth2 for authentication. When launched for the first time, a link will be generated
that can be opened in any browser. After logging in on the displayed page, you can start to use
`ncspot`. The OAuth2 flow is the only supported one, as username/password authentication has been
removed by Spotify.

The credentials are stored in `librespot/credentials.json` in the user's cache directory. Run
`ncspot info` to show the location of this directory.

The `logout` command can be used to remove cached credentials. See
[Vim-Like Commands](#vim-like-commands).
