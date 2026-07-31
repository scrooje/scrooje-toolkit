use std::io::{IsTerminal, Read, Write};

use clap::Parser;
use scrooje_core::entry;
use secrecy::ExposeSecret;

mod beancount;
mod client;

#[derive(Parser)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Download and decrypt blobs.
    Blobs(BlobsArgs),
    /// Export scrooje entries to another format.
    Export(ExportArgs),
}

#[derive(clap::Args)]
struct BlobsArgs {
    #[command(subcommand)]
    command: BlobsCommand,
}

#[derive(clap::Subcommand)]
enum BlobsCommand {
    /// Download and store blobs on the filesystem.
    Download {
        /// Base URL of the API server.
        #[arg(long, default_value = "https://app.scroo.je/")]
        base_url: reqwest::Url,
        /// Whether to attempt to extract blobs after downloading. The output
        /// in this case will be sent through stdout.
        #[arg(long, default_value = "false")]
        extract: bool,
        /// Output directory to write blobs files to. Required when `extract` is
        /// not set.
        #[arg(long)]
        output_dir: Option<std::path::PathBuf>,
        /// Whether to skip TLS certificate verification.
        #[arg(long, default_value = "false")]
        skip_verify: bool,
        /// Whether to authenticate using passkey.
        #[arg(long, default_value = "false")]
        use_passkey: bool,
        /// Username for password-based authentication. Password will be prompted.
        #[arg(long)]
        username: String,
        /// Book UUID to download blobs for.
        book_uuid: uuid::Uuid,
    },
    /// Extract JSON entries from blobs stored on the filesystem.
    Extract {
        /// Path to keyfile to get the book root key from. Otherwise the key
        /// will be prompted.
        #[arg(long)]
        key_file: Option<std::path::PathBuf>,
        /// Paths to blobs. The basename must match the name on the server side.
        blob: Vec<std::path::PathBuf>,
    },
}

#[derive(clap::Args)]
struct ExportArgs {
    /// The format to export the entries to. Can be one of: beancount.
    #[arg(long)]
    format: ExportFormat,
    /// Output directory to write the data to.
    #[arg(long)]
    output_dir: std::path::PathBuf,
    /// An input file containing JSON entries, or '-' for stdin.
    input: std::path::PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ExportFormat {
    Beancount,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Beancount => f.write_str("beancount"),
        }
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = UnrecognizedExportFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "beancount" => Ok(ExportFormat::Beancount),
            _ => Err(UnrecognizedExportFormatError(s.to_owned())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
struct UnrecognizedExportFormatError(String);

impl std::fmt::Display for UnrecognizedExportFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unrecognized export format: {}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
enum KeyFileError {
    #[error("invalid line in keyfile: {0:?}")]
    InvalidLine(String),
    #[error("duplicate book uuid in keyfile")]
    DuplicateUuid,
    #[error("duplicate key in keyfile")]
    DuplicateKey,
    #[error("book uuid is missing from keyfile")]
    MissingUuid,
    #[error("book key is missing from keyfile")]
    MissingKey,
    #[error("invalid uuid in keyfile: {0}")]
    InvalidUuid(#[from] uuid::Error),
    #[error("invalid key in keyfile: {0}")]
    InvalidKey(#[from] scrooje_crypto::HexError),
}

fn parse_keyfile(content: &str) -> Result<(uuid::Uuid, secrecy::SecretSlice<u8>), KeyFileError> {
    let mut book_uuid = None;
    let mut root_key = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| KeyFileError::InvalidLine(line.to_owned()))?;

        match key.trim() {
            "uuid" if book_uuid.is_some() => return Err(KeyFileError::DuplicateUuid),
            "uuid" => book_uuid = Some(value.trim().parse()?),
            "key" if root_key.is_some() => return Err(KeyFileError::DuplicateKey),
            "key" => root_key = Some(scrooje_crypto::decode_hex(value.trim())?.into()),
            _ => return Err(KeyFileError::InvalidLine(line.to_owned())),
        }
    }

    Ok((
        book_uuid.ok_or(KeyFileError::MissingUuid)?,
        root_key.ok_or(KeyFileError::MissingKey)?,
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    match cli.command {
        Command::Blobs(args) => match args.command {
            BlobsCommand::Download {
                base_url,
                book_uuid,
                extract,
                output_dir,
                skip_verify,
                use_passkey,
                username,
            } => {
                if !extract && output_dir.is_none() {
                    eprintln!("--output-dir is required when --extract is not set");
                    std::process::exit(2);
                }

                const MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");
                const MINOR_VERSION: &str = env!("CARGO_PKG_VERSION_MINOR");

                let root_key = if !use_passkey {
                    let password: secrecy::SecretString = inquire::Password::new("Enter password:")
                        .without_confirmation()
                        .prompt()?
                        .into();
                    log::info!("deriving keys");
                    scrooje_crypto::RootKey::from_password(
                        &username,
                        password.expose_secret(),
                        scrooje_crypto::PbkdfParams::V1,
                    )?
                } else {
                    let pin: secrecy::SecretString = inquire::Password::new("Enter device PIN:")
                        .without_confirmation()
                        .prompt()?
                        .into();
                    scrooje_crypto::RootKey::from_passkey(
                        base_url
                            .domain()
                            .ok_or("base url must contain a domain name".to_owned())?,
                        &username,
                        pin.expose_secret(),
                    )?
                };

                let http_client = reqwest::Client::builder()
                    .danger_accept_invalid_certs(skip_verify)
                    .danger_accept_invalid_hostnames(skip_verify)
                    .user_agent(format!("scrooje-cli/{MAJOR_VERSION}.{MINOR_VERSION}"))
                    .use_rustls_tls()
                    .build()?;

                let client = client::Client::new(base_url, http_client, root_key);

                log::info!("logging in");
                let jwt = client.login(&username).await?;

                log::info!("fetching blobs");
                let blobs = client.get_blobs(&jwt, book_uuid).await?;
                log::info!("fetched {} blobs", blobs.len());
                if extract {
                    log::info!("fetching book root key");
                    let secret_key = client.get_book_secret_key(&jwt, book_uuid).await?;

                    log::info!("extracting blobs contents");
                    let mut stdout = std::io::stdout().lock();
                    for blob in blobs {
                        let bytes =
                            scrooje_crypto::blob::extract(&secret_key, &blob.name, &blob.data)?;
                        stdout.write_all(bytes.as_bytes())?;
                        writeln!(&mut stdout)?;
                    }
                    stdout.flush()?;
                } else {
                    let output_dir = output_dir.expect("output_dir should be set at this point");
                    if !output_dir.exists() {
                        std::fs::create_dir_all(&output_dir)?;
                    }

                    log::info!("writing blobs to filesystem");
                    for blob in blobs {
                        std::fs::write(output_dir.join(&blob.name), &blob.data)?;
                    }
                }
                log::info!("download succeeded");
            }
            BlobsCommand::Extract { key_file, blob } => {
                if blob.is_empty() {
                    eprintln!("at least 1 blob is required");
                    std::process::exit(2);
                }

                log::info!("reading blobs");
                let blobs: Result<Vec<_>, _> = blob
                    .iter()
                    .map(|path| -> Result<_, std::io::Error> {
                        Ok((
                            path.file_name()
                                .and_then(|file_name| file_name.to_str())
                                .unwrap_or_default()
                                .to_owned(),
                            std::fs::read(path)?,
                        ))
                    })
                    .collect();

                let (_, root_key) = if let Some(key_file) = key_file {
                    let content: secrecy::SecretString = std::fs::read_to_string(&key_file)?.into();
                    let (book_uuid, key_bytes) = parse_keyfile(content.expose_secret())?;
                    (
                        book_uuid,
                        scrooje_crypto::BookSecretKey::from_slice(key_bytes.expose_secret())?,
                    )
                } else {
                    let book_uuid: uuid::Uuid = inquire::Text::new("Enter book UUID:")
                        .with_validator(|input: &str| {
                            if input.parse::<uuid::Uuid>().is_ok() {
                                Ok(inquire::validator::Validation::Valid)
                            } else {
                                Ok(inquire::validator::Validation::Invalid(
                                    "Invalid uuid.".into(),
                                ))
                            }
                        })
                        .prompt()?
                        .parse()?;
                    let key_hex: secrecy::SecretString =
                        inquire::Password::new("Enter book secret key:")
                            .without_confirmation()
                            .prompt()?
                            .into();
                    (
                        book_uuid,
                        scrooje_crypto::BookSecretKey::from_hex(key_hex.expose_secret())?,
                    )
                };

                log::info!("extracting blobs contents");
                let mut stdout = std::io::stdout().lock();
                for (name, data) in blobs? {
                    let bytes = scrooje_crypto::blob::extract(&root_key, &name, &data)?;
                    stdout.write_all(bytes.as_bytes())?;
                    writeln!(&mut stdout)?;
                }
                stdout.flush()?;
            }
        },
        Command::Export(args) => {
            let content = if args.input.as_os_str() == "-" {
                if std::io::stdin().is_terminal() {
                    eprintln!("stdin must not be a tty");
                    std::process::exit(2);
                }

                let mut content = String::new();
                std::io::stdin().lock().read_to_string(&mut content)?;
                content
            } else {
                std::fs::read_to_string(&args.input)?
            };

            log::info!("preparing data for export");
            let results: Vec<_> = content
                .lines()
                .enumerate()
                .map(|(lineno, line)| {
                    entry::Entry::from_json(line.as_bytes()).map_err(|err| (lineno + 1, err))
                })
                .collect();

            let mut had_error = false;
            for (lineno, err) in results.iter().filter_map(|r| r.as_ref().err()) {
                log::error!("failed to parse entry on line {lineno}: {err}");
                had_error = true;
            }
            if had_error {
                return Err("failed to parse one or more entries".into());
            }
            let entries: Vec<_> = results.into_iter().map(Result::unwrap).collect();

            if !args.output_dir.exists() {
                std::fs::create_dir_all(&args.output_dir)?;
            }
            match args.format {
                ExportFormat::Beancount => {
                    let ctx = beancount::ExportContext::new(&entries)?;
                    beancount::export(&ctx, &args.output_dir)?;
                }
            }
            log::info!("export succeeded");
        }
    }

    Ok(())
}
