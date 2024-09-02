// a CLI to execute the same methods like in the RPC server

use ckb_jsonrpc_types::{OutPoint, Script, TransactionView};
use ckb_types::H256;
use jsonrpsee::tracing;
use jsonrpsee::types::ErrorObjectOwned;
use std::str::FromStr;

mod error;
mod rpc_client;
mod ssri_vm;
mod types;
use clap::{Arg, ArgAction, Command};

use error::Error;
use rpc_client::RpcClient;
use types::{CellOutputWithData, Hex};

use ssri_vm::execute_riscv_binary;

fn main() {
    let matches = Command::new("SSRI CLI")
        .version("1.0")
        .author("Your Name")
        .about("CLI for executing SSRI scripts")
        .subcommand(
            Command::new("run")
                .about("Run a script")
                .arg(Arg::new("tx_hash").required(true).help("Transaction hash"))
                .arg(Arg::new("index").required(true).help("Cell index"))
                .arg(
                    Arg::new("args")
                        .action(ArgAction::Append)
                        .help("Script arguments"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let tx_hash = matches
            .get_one::<String>("tx_hash")
            .expect("Transaction hash is required");
        let tx_hash = if let Some(stripped) = tx_hash.strip_prefix("0x") {
            H256::from_str(stripped)
        } else {
            H256::from_str(tx_hash)
        }
        .expect("Invalid transaction hash");
        let index = matches
            .get_one::<String>("index")
            .unwrap()
            .parse::<u32>()
            .expect("Invalid index");
        let args: Vec<Hex> = matches
            .get_many::<String>("args")
            .map(|values| values.map(|v| Hex::from(v.as_str())).collect())
            .unwrap_or_default();

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                match run_script(tx_hash, index, args, None, None, None).await {
                    Ok(result) => match result {
                        Some(hex) => println!("Script execution result: {:?}", hex.clone()),
                        None => println!("Script execution completed without a return value"),
                    },
                    Err(e) => eprintln!("Error executing script: {}", e),
                }
            });
    }
}

async fn run_script(
    tx_hash: H256,
    index: u32,
    args: Vec<Hex>,
    script: Option<Script>,
    cell: Option<CellOutputWithData>,
    tx: Option<TransactionView>,
) -> Result<Option<Hex>, ErrorObjectOwned> {
    let rpc = RpcClient::new("https://testnet.ckbapp.dev/"); // Assuming default CKB node RPC URL

    let ssri_cell = rpc
        .get_live_cell(
            &OutPoint {
                tx_hash: tx_hash.0.into(),
                index: index.into(),
            },
            true,
        )
        .await?;

    tracing::info!("Running script on {tx_hash}:{index} with args {args:?}");

    let ssri_binary = ssri_cell
        .cell
        .ok_or(Error::InvalidRequest("Cell not found"))?
        .data
        .ok_or(Error::InvalidRequest("Cell doesn't have data"))?
        .content
        .into_bytes();

    let args = args.into_iter().map(|v| v.hex.into()).collect();
    let script = script.map(Into::into);
    let cell = cell.map(Into::into);
    let tx = tx.map(|v| v.inner.into());

    Ok(execute_riscv_binary(rpc.clone(), ssri_binary, args, script, cell, tx)?.map(|v| v.into()))
}
