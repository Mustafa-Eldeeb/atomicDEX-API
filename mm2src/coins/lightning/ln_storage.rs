use super::*;
use common::mm_ctx::MmArc;
use lightning::routing::network_graph::NetworkGraph;
use lightning::routing::scoring::Scorer;
use lightning::util::ser::{Readable, Writeable};
use secp256k1::PublicKey;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn my_ln_data_dir(ctx: &MmArc, ticker: &str) -> PathBuf { ctx.dbdir().join("LIGHTNING").join(ticker) }

pub fn nodes_data_path(ctx: &MmArc, ticker: &str) -> PathBuf { my_ln_data_dir(ctx, ticker).join("channel_nodes_data") }

pub fn network_graph_path(ctx: &MmArc, ticker: &str) -> PathBuf { my_ln_data_dir(ctx, ticker).join("network_graph") }

pub fn scorer_path(ctx: &MmArc, ticker: &str) -> PathBuf { my_ln_data_dir(ctx, ticker).join("scorer") }

pub fn last_request_id_path(ctx: &MmArc, ticker: &str) -> PathBuf {
    my_ln_data_dir(ctx, ticker).join("LAST_REQUEST_ID")
}

fn pubkey_and_addr_from_str(pubkey_str: &str, addr_str: &str) -> ConnectToNodeResult<(PublicKey, SocketAddr)> {
    // TODO: support connection to onion addresses
    let addr = addr_str
        .to_socket_addrs()
        .map(|mut r| r.next())
        .map_to_mm(|e| ConnectToNodeError::ParseError(e.to_string()))?
        .ok_or_else(|| ConnectToNodeError::ParseError(format!("Couldn't parse {} into a socket address", addr_str)))?;

    let pubkey = PublicKey::from_str(pubkey_str).map_to_mm(|e| ConnectToNodeError::ParseError(e.to_string()))?;

    Ok((pubkey, addr))
}

pub fn parse_node_info(node_pubkey_and_ip_addr: String) -> ConnectToNodeResult<(PublicKey, SocketAddr)> {
    let mut pubkey_and_addr = node_pubkey_and_ip_addr.split('@');

    let pubkey = pubkey_and_addr.next().ok_or_else(|| {
        ConnectToNodeError::ParseError(format!(
            "Incorrect node id format for {}. The format should be `pubkey@host:port`",
            node_pubkey_and_ip_addr
        ))
    })?;

    let node_addr_str = pubkey_and_addr.next().ok_or_else(|| {
        ConnectToNodeError::ParseError(format!(
            "Incorrect node id format for {}. The format should be `pubkey@host:port`",
            node_pubkey_and_ip_addr
        ))
    })?;

    let (pubkey, node_addr) = pubkey_and_addr_from_str(pubkey, node_addr_str)?;
    Ok((pubkey, node_addr))
}

pub fn read_nodes_data_from_file(path: &Path) -> ConnectToNodeResult<HashMap<PublicKey, SocketAddr>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let mut nodes_data = HashMap::new();
    let file = File::open(path).map_to_mm(|e| ConnectToNodeError::IOError(e.to_string()))?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_to_mm(|e| ConnectToNodeError::IOError(e.to_string()))?;
        let (pubkey, socket_addr) = parse_node_info(line)?;
        nodes_data.insert(pubkey, socket_addr);
    }
    Ok(nodes_data)
}

pub fn save_node_data_to_file(path: &Path, node_info: &str) -> ConnectToNodeResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_to_mm(|e| ConnectToNodeError::IOError(e.to_string()))?;
    file.write_all(format!("{}\n", node_info).as_bytes())
        .map_to_mm(|e| ConnectToNodeError::IOError(e.to_string()))
}

pub fn save_network_graph_to_file(path: &Path, network_graph: &NetworkGraph) -> EnableLightningResult<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .map_to_mm(|e| EnableLightningError::IOError(e.to_string()))?;
    network_graph
        .write(&mut BufWriter::new(file))
        .map_to_mm(|e| EnableLightningError::IOError(e.to_string()))
}

pub fn read_network_graph_from_file(path: &Path) -> EnableLightningResult<NetworkGraph> {
    let file = File::open(path).map_to_mm(|e| EnableLightningError::IOError(e.to_string()))?;
    NetworkGraph::read(&mut BufReader::new(file)).map_to_mm(|e| EnableLightningError::IOError(e.to_string()))
}

pub fn save_scorer_to_file(path: &Path, scorer: &Scorer) -> EnableLightningResult<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .map_to_mm(|e| EnableLightningError::IOError(e.to_string()))?;
    scorer
        .write(&mut BufWriter::new(file))
        .map_to_mm(|e| EnableLightningError::IOError(e.to_string()))
}

pub fn read_scorer_from_file(path: &Path) -> EnableLightningResult<Scorer> {
    let file = File::open(path).map_to_mm(|e| EnableLightningError::IOError(e.to_string()))?;
    Scorer::read(&mut BufReader::new(file)).map_to_mm(|e| EnableLightningError::IOError(e.to_string()))
}

pub fn save_last_request_id_to_file(path: &Path, last_request_id: u64) -> OpenChannelResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .map_to_mm(|e| OpenChannelError::IOError(e.to_string()))?;
    file.write_all(format!("{}", last_request_id).as_bytes())
        .map_to_mm(|e| OpenChannelError::IOError(e.to_string()))
}

pub fn read_last_request_id_from_file(path: &Path) -> OpenChannelResult<u64> {
    if !path.exists() {
        return MmError::err(OpenChannelError::InvalidPath(format!(
            "Path {} does not exist",
            path.display()
        )));
    }
    let mut file = File::open(path).map_to_mm(|e| OpenChannelError::IOError(e.to_string()))?;
    let mut contents = String::new();
    let _ = file.read_to_string(&mut contents);
    contents
        .parse::<u64>()
        .map_to_mm(|e| OpenChannelError::IOError(e.to_string()))
}
