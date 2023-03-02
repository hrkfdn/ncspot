use std::path::PathBuf;
use std::{env, io};

use clap::builder::PathBufValueParser;
use clap::error::{Error, ErrorKind};
use clap::ArgMatches;
use clap_complete::Shell;
use ncspot::{AUTHOR, BIN_NAME};

enum XTaskSubcommand {
    GenerateManpage,
    GenerateShellCompletionScript,
}

impl TryFrom<&ArgMatches> for XTaskSubcommand {
    type Error = clap::Error;

    fn try_from(value: &ArgMatches) -> Result<Self, Self::Error> {
        if let Some(subcommand) = value.subcommand() {
            match subcommand.0 {
                "generate-manpage" => Ok(XTaskSubcommand::GenerateManpage),
                "generate-shell-completion" => Ok(XTaskSubcommand::GenerateShellCompletionScript),
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
cargo workflox. Xtask's are defined as a separate package and can be used for all kinds of
automation.
        ",
        )
        .subcommands([
            clap::Command::new("generate-manpage")
                .visible_alias("gm")
                .args([clap::Arg::new("output")
                    .short('o')
                    .long("output")
                    .value_name("PATH")
                    .help("The output path for the manpage.")
                    .value_parser(PathBufValueParser::new())
                    .required(true)])
                .about("Automatic manpage generation"),
            clap::Command::new("generate-shell-completion")
                .visible_alias("gsc")
                .args([clap::Arg::new("shell")
                    .short('s')
                    .long("shell")
                    .default_value("bash")
                    .help("The shell for which completion should be generated (default = bash).")])
                .about("Automatic shell completion generation."),
        ]);

    let program_parsed_arguments = arguments_model.get_matches();

    let parsed_subcommand = XTaskSubcommand::try_from(&program_parsed_arguments)?;

    let subcommand_parsed_arguments = program_parsed_arguments.subcommand().unwrap().1;

    match parsed_subcommand {
        XTaskSubcommand::GenerateManpage => generate_manpage(subcommand_parsed_arguments),
        XTaskSubcommand::GenerateShellCompletionScript => {
            generate_completion_script(subcommand_parsed_arguments)
        }
    }
}

fn generate_manpage(subcommand_arguments: &ArgMatches) -> Result<(), DynError> {
    let cmd = ncspot::program_arguments();

    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;

    std::fs::write(
        subcommand_arguments
            .get_one::<PathBuf>("output")
            .unwrap()
            .join(format!("{}.1", BIN_NAME)),
        buffer,
    )?;

    Ok(())
}

fn generate_completion_script(subcommand_arguments: &ArgMatches) -> Result<(), DynError> {
    let shell = match subcommand_arguments
        .get_one::<String>("shell")
        .unwrap()
        .as_str()
    {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "elvish" => Shell::Elvish,
        "powershell" => Shell::PowerShell,
        _ => Shell::Bash,
    };
    clap_complete::generate(
        shell,
        &mut ncspot::program_arguments(),
        ncspot::BIN_NAME,
        &mut io::stdout(),
    );
    Ok(())
}
