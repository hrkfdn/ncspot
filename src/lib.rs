use clap::builder::PathBufValueParser;
use librespot_playback::audio_backend;

pub const AUTHOR: &str = "Henrik Friedrichsen <henrik@affekt.org> and contributors";
pub const BIN_NAME: &str = "ncspot";
pub const CONFIGURATION_FILE_NAME: &str = "config.toml";
pub const USER_STATE_FILE_NAME: &str = "userstate.cbor";

/// Return the [Command](clap::Command) that models the program's command line arguments. The
/// command can be used to parse the actual arguments passed to the program, or to automatically
/// generate a man page using clap's mangen package.
pub fn program_arguments() -> clap::Command {
    let backends = {
        let backends: Vec<&str> = audio_backend::BACKENDS.iter().map(|b| b.0).collect();
        format!("Audio backends: {}", backends.join(", "))
    };

    clap::Command::new("ncspot")
        .version(env!("VERSION"))
        .author(AUTHOR)
        .about("cross-platform ncurses Spotify client")
        .after_help(backends)
        .arg(
            clap::Arg::new("debug")
                .short('d')
                .long("debug")
                .value_name("FILE")
                .value_parser(PathBufValueParser::new())
                .help("Enable debug logging to the specified file"),
        )
        .arg(
            clap::Arg::new("basepath")
                .short('b')
                .long("basepath")
                .value_name("PATH")
                .value_parser(PathBufValueParser::new())
                .help("custom basepath to config/cache files"),
        )
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Filename of config file in basepath")
                .default_value(CONFIGURATION_FILE_NAME),
        )
        .arg(
            clap::Arg::new("playlist")
                .long("playlist")
                .value_name("URL_OR_ID")
                .help("Start playback from the Spotify playlist URL or ID"),
        )
        .subcommands([clap::Command::new("info").about("Print platform information like paths")])
}

#[cfg(test)]
mod tests {
    use super::program_arguments;

    #[test]
    fn test_program_arguments_accepts_playlist() {
        let matches = program_arguments()
            .try_get_matches_from([
                "ncspot",
                "--playlist",
                "https://open.spotify.com/playlist/6UUCMxk575eDTwSWa0qQhB?si=fa799fec9a404660",
            ])
            .unwrap();

        assert_eq!(
            matches.get_one::<String>("playlist").map(String::as_str),
            Some("https://open.spotify.com/playlist/6UUCMxk575eDTwSWa0qQhB?si=fa799fec9a404660")
        );
    }
}
