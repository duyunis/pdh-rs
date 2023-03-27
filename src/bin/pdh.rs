use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};

use pdh_rs::common::{consts, version};


#[derive(Parser)]
#[clap(name = "pdh")]
struct Cmd {
    #[clap(subcommand)]
    pdh: Option<PdhCmd>,

    /// Display the version
    #[clap(short, long, action = ArgAction::SetTrue)]
    version: bool,

    /// Show debug log
    #[clap(long, action = ArgAction::SetTrue)]
    debug: bool,
}

#[derive(Subcommand)]
enum PdhCmd {
    /// Send file(s), or folder (see options with pdh send -h)
    Send(SendCmd),

    /// Receive file(s), or folder (see options with pdh recv -h)
    Recv(ReceiveCmd),

    /// Start your own relay (see options with pdh relay -h)
    Relay(RelayCmd),
}

#[derive(Parser)]
struct SendCmd {
    /// Send file(s), folder share code to receive. If not set will auto-generated
    #[clap(short = 'c', long)]
    share_code: Option<String>,

    /// zip file(s) or folder before sending
    #[clap(long, action = ArgAction::SetTrue)]
    zip: Option<bool>,

    /// relay address (default: public relay)
    #[clap(long)]
    relay: Option<String>,
}

#[derive(Parser)]
struct ReceiveCmd {
}

#[derive(Parser)]
struct RelayCmd {
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum RpcData {
    Config,
    Platform,
    TapTypes,
    Cidr,
    Groups,
    Acls,
    Segments,
    Version,
}

const VERSION_INFO: &'static version::VersionInfo = &version::VersionInfo {
    name: "PDH",
    version: consts::PDH_VERSION,
    compiler: env!("RUSTC_VERSION"),
    compile_time: env!("COMPILE_TIME"),
};

fn main() -> Result<()> {
    let cmd = Cmd::parse();

    if cmd.version {
        println!("{}", VERSION_INFO);
        return Ok(());
    }

    if let Some(pdh_cmd) = cmd.pdh {
        match pdh_cmd {
            PdhCmd::Send(send) => {

            }
            PdhCmd::Recv(recv) => {

            }
            PdhCmd::Relay(relay) => {

            }
        }
    } else {
        let mut cmd = Cmd::command();
        cmd.print_help().unwrap();
    }
    Ok(())
}