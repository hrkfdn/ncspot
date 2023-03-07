use std::path::PathBuf;
use std::{env, fs};

use clap::builder::PathBufValueParser;
use clap::error::{Error, ErrorKind};
use clap::ArgMatches;
use clap_complete::Shell;
use ncspot::{AUTHOR, BIN_NAME};

static DEFAULT_OUTPUT_DIRECTORY: &str = "misc";

enum XTaskSubcommand {
    GenerateManpage,
    GenerateShellCompletion,
}

impl TryFrom<&ArgMatches> for XTaskSubcommand {
    type Error = clap::Error;

    fn try_from(value: &ArgMatches) -> Result<Self, Self::Error> {
        if let Some(subcommand) = value.subcommand() {
            match subcommand.0 {
                "generate-manpage" => Ok(XTaskSubcommand::GenerateManpage),
                "generate-shell-completion" => Ok(XTaskSubcommand::GenerateShellCompletion),
                _ => Err(Error::new(clap::error::ErrorKind::InvalidSubcommand)),
            }
        } else {
            Err(Error::new(ErrorKind::MissingSubcommand))
        }
    }
}

type DynError = Box<dyn std::error::Error>;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let arguments_model = clap::Command::new("cargo xtask")
        .version(env!("CARGO_PKG_VERSION"))
        .author(AUTHOR)
        .about("Automation using the cargo xtask convention.")
        .arg_required_else_help(true)
        .bin_name("cargo xtask")
        .disable_version_flag(true)
        .long_about(
            "
Cargo xtask is a convention that allows easy integration of third party commands into the regular
cargo workflow. Xtask's are defined as a separate package and can be used for all kinds of
automation.",
        )
        .subcommands([
            clap::Command::new("generate-manpage")
                .visible_alias("gm")
                .args([clap::Arg::new("output")
                    .short('o')
                    .long("output")
                    .value_name("PATH")
                    .default_value("misc")
                    .help("Output directory for the generated man page.")
                    .value_parser(PathBufValueParser::new())])
                .about("Automatic man page generation."),
            clap::Command::new("generate-shell-completion")
                .visible_alias("gsc")
                .args([
                    clap::Arg::new("shells")
                        .short('s')
                        .long("shells")
                        .value_name("SHELLS")
                        .default_values(["bash", "zsh", "fish"])
                        .value_delimiter(',')
                        .help("The shells for which completion should be generated."),
                    clap::Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("PATH")
                        .default_value("misc")
                        .help("Output directory for the generated completion script.")
                        .value_parser(PathBufValueParser::new()),
                ])
                .about("Automatic shell completion generation.")
                .long_about(
                    "
Automatic shell completion generation.
Supported shells: bash,zsh,fish,elvish,powershell",
                ),
        ]);

    let program_parsed_arguments = arguments_model.get_matches();

    let parsed_subcommand = XTaskSubcommand::try_from(&program_parsed_arguments)?;

    let subcommand_parsed_arguments = program_parsed_arguments.subcommand().unwrap().1;

    match parsed_subcommand {
        XTaskSubcommand::GenerateManpage => generate_manpage(subcommand_parsed_arguments),
        XTaskSubcommand::GenerateShellCompletion => {
            generate_shell_completion(subcommand_parsed_arguments)
        }
    }
}

fn generate_manpage(subcommand_arguments: &ArgMatches) -> Result<(), DynError> {
    let default_output_directory = PathBuf::from(DEFAULT_OUTPUT_DIRECTORY);
    let output_directory = subcommand_arguments
        .get_one::<PathBuf>("output")
        .unwrap_or(&default_output_directory);
    let cmd = ncspot::program_arguments();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();

    if *output_directory == default_output_directory {
        fs::create_dir_all(DEFAULT_OUTPUT_DIRECTORY)?;
    }

    man.render(&mut buffer)?;
    std::fs::write(output_directory.join("ncspot.1"), buffer)?;

    Ok(())
}

fn generate_shell_completion(subcommand_arguments: &ArgMatches) -> Result<(), DynError> {
    let default_output_directory = PathBuf::from(DEFAULT_OUTPUT_DIRECTORY);
    let output_directory = subcommand_arguments
        .get_one::<PathBuf>("output")
        .unwrap_or(&default_output_directory);
    let shells = subcommand_arguments
        .get_many::<String>("shells")
        .map(|shells| {
            shells
                .map(|shell| match shell.as_str() {
                    "bash" => Shell::Bash,
                    "zsh" => Shell::Zsh,
                    "fish" => Shell::Fish,
                    "elvish" => Shell::Elvish,
                    "powershell" => Shell::PowerShell,
                    _ => {
                        eprintln!("Unrecognized shell: {}", shell);
                        std::process::exit(-1);
                    }
                })
                .collect()
        })
        .unwrap_or(vec![Shell::Bash, Shell::Zsh, Shell::Fish]);

    if *output_directory == default_output_directory {
        fs::create_dir_all(DEFAULT_OUTPUT_DIRECTORY)?;
    }

    for shell in shells {
        clap_complete::generate_to(
            shell,
            &mut ncspot::program_arguments(),
            BIN_NAME,
            output_directory,
        )?;
    }

    Ok(())
}
