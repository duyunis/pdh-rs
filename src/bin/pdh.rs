use std::sync::Arc;
use anyhow::Result;
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use tokio::runtime::Builder;

use pdh_rs::common::{consts, version};
use pdh_rs::relay::relay;
use pdh_rs::send::sender::{Sender, SenderOptions};

#[derive(Parser, Debug)]
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

#[derive(Subcommand, Debug)]
enum PdhCmd {
    /// Send file(s), or folder (see options with pdh send -h)
    Send(SendCmd),

    /// Receive file(s), or folder (see options with pdh recv -h)
    Recv(ReceiveCmd),

    /// Start your own relay (see options with pdh relay -h)
    Relay(RelayCmd),
}

#[derive(Parser, Debug)]
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

    /// file(s) to send
    #[clap(required = true, num_args = 1..)]
    files: Vec<String>,
}

#[derive(Parser, Debug)]
struct ReceiveCmd {}

#[derive(Parser, Debug)]
struct RelayCmd {
    ///  relay host
    #[clap(long, default_value = "0.0.0.0")]
    host: String,

    /// relay port
    #[clap(short = 'p', long, default_value = "6880")]
    port: u16,
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
        let runtime = Arc::new(Builder::new_multi_thread()
            .worker_threads(16)
            .enable_all()
            .build()
            .unwrap());

        match pdh_cmd {
            PdhCmd::Send(send) => {
                let sender_options = SenderOptions::new(send.share_code, send.zip.unwrap(), send.relay, send.files);
                let mut sender = Sender::new(runtime.clone(), sender_options);
                sender.send()?;
            }
            PdhCmd::Recv(recv) => {}
            PdhCmd::Relay(relay) => {
                let relay = relay::Relay::new(relay.host, relay.port, runtime.clone());
                relay.run()?;
            }
        }
    } else {
        let mut cmd = Cmd::command();
        cmd.print_help().unwrap();
    }
    Ok(())
}
