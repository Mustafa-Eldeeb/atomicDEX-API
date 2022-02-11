//! Utilities that handle persisting Rust-Lightning data to disk via standard filesystem APIs.

mod util;

extern crate bitcoin;
extern crate libc;
extern crate lightning;

use crate::util::DiskWriteable;
use bitcoin::hash_types::{BlockHash, Txid};
use bitcoin::hashes::hex::{FromHex, ToHex};
use lightning::chain;
use lightning::chain::chaininterface::{BroadcasterInterface, FeeEstimator};
use lightning::chain::chainmonitor;
use lightning::chain::channelmonitor::{ChannelMonitor, ChannelMonitorUpdate};
use lightning::chain::keysinterface::{KeysInterface, Sign};
use lightning::chain::transaction::OutPoint;
use lightning::ln::channelmanager::ChannelManager;
use lightning::util::logger::Logger;
use lightning::util::ser::{ReadableArgs, Writeable};
use std::fs;
use std::io::{Cursor, Error};
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// FilesystemPersister persists channel data on disk, where each channel's
/// data is stored in a file named after its funding outpoint.
///
/// Warning: this module does the best it can with calls to persist data, but it
/// can only guarantee that the data is passed to the drive. It is up to the
/// drive manufacturers to do the actual persistence properly, which they often
/// don't (especially on consumer-grade hardware). Therefore, it is up to the
/// user to validate their entire storage stack, to ensure the writes are
/// persistent.
/// Corollary: especially when dealing with larger amounts of money, it is best
/// practice to have multiple channel data backups and not rely only on one
/// FilesystemPersister.
pub struct FilesystemPersister {
    path_to_channel_data: String,
    path_to_backup: Option<String>,
}

impl<Signer: Sign> DiskWriteable for ChannelMonitor<Signer> {
    fn write_to_file(&self, writer: &mut fs::File) -> Result<(), Error> { self.write(writer) }
}

impl<Signer: Sign, M: Deref, T: Deref, K: Deref, F: Deref, L: Deref> DiskWriteable
    for ChannelManager<Signer, M, T, K, F, L>
where
    M::Target: chain::Watch<Signer>,
    T::Target: BroadcasterInterface,
    K::Target: KeysInterface<Signer = Signer>,
    F::Target: FeeEstimator,
    L::Target: Logger,
{
    fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error> { self.write(writer) }
}

impl FilesystemPersister {
    /// Initialize a new FilesystemPersister and set the path to the individual channels'
    /// files.
    pub fn new(path_to_channel_data: String, path_to_backup: Option<String>) -> Self {
        Self {
            path_to_channel_data,
            path_to_backup,
        }
    }

    /// Get the directory which was provided when this persister was initialized.
    pub fn get_data_dir(&self) -> String { self.path_to_channel_data.clone() }

    pub(crate) fn path_to_monitor_data(&self) -> PathBuf {
        let mut path = PathBuf::from(self.path_to_channel_data.clone());
        path.push("monitors");
        path
    }

    pub(crate) fn path_to_monitor_data_backup(&self) -> Option<PathBuf> {
        if let Some(backup_path) = self.path_to_backup.clone() {
            let mut path = PathBuf::from(backup_path);
            path.push("monitors");
            return Some(path);
        }
        None
    }

    /// Writes the provided `ChannelManager` to the path provided at `FilesystemPersister`
    /// initialization, within a file called "manager".
    pub fn persist_manager<Signer: Sign, M: Deref, T: Deref, K: Deref, F: Deref, L: Deref>(
        data_dir: String,
        backup_dir: Option<String>,
        manager: &ChannelManager<Signer, M, T, K, F, L>,
    ) -> Result<(), std::io::Error>
    where
        M::Target: chain::Watch<Signer>,
        T::Target: BroadcasterInterface,
        K::Target: KeysInterface<Signer = Signer>,
        F::Target: FeeEstimator,
        L::Target: Logger,
    {
        let path = PathBuf::from(data_dir);
        util::write_to_file(path, "manager".to_string(), manager)?;
        if let Some(path) = backup_dir {
            let backup_path = PathBuf::from(path);
            util::write_to_file(backup_path, "manager".to_string(), manager)?;
        }
        Ok(())
    }

    /// Read `ChannelMonitor`s from disk.
    pub fn read_channelmonitors<Signer: Sign, K: Deref>(
        &self,
        keys_manager: K,
    ) -> Result<Vec<(BlockHash, ChannelMonitor<Signer>)>, std::io::Error>
    where
        K::Target: KeysInterface<Signer = Signer> + Sized,
    {
        let path = self.path_to_monitor_data();
        if !Path::new(&path).exists() {
            return Ok(Vec::new());
        }
        let mut res = Vec::new();
        for file_option in fs::read_dir(path).unwrap() {
            let file = file_option.unwrap();
            let owned_file_name = file.file_name();
            let filename = owned_file_name.to_str();
            if filename.is_none() || !filename.unwrap().is_ascii() || filename.unwrap().len() < 65 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid ChannelMonitor file name",
                ));
            }

            let txid = Txid::from_hex(filename.unwrap().split_at(64).0);
            if txid.is_err() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid tx ID in filename",
                ));
            }

            let index = filename.unwrap().split_at(65).1.parse();
            if index.is_err() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid tx index in filename",
                ));
            }

            let contents = fs::read(&file.path())?;
            let mut buffer = Cursor::new(&contents);
            match <(BlockHash, ChannelMonitor<Signer>)>::read(&mut buffer, &*keys_manager) {
                Ok((blockhash, channel_monitor)) => {
                    if channel_monitor.get_funding_txo().0.txid != txid.unwrap()
                        || channel_monitor.get_funding_txo().0.index != index.unwrap()
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "ChannelMonitor was stored in the wrong file",
                        ));
                    }
                    res.push((blockhash, channel_monitor));
                },
                Err(e) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to deserialize ChannelMonitor: {}", e),
                    ))
                },
            }
        }
        Ok(res)
    }
}

impl<ChannelSigner: Sign> chainmonitor::Persist<ChannelSigner> for FilesystemPersister {
    // TODO: We really need a way for the persister to inform the user that its time to crash/shut
    // down once these start returning failure.
    // A PermanentFailure implies we need to shut down since we're force-closing channels without
    // even broadcasting!

    fn persist_new_channel(
        &self,
        funding_txo: OutPoint,
        monitor: &ChannelMonitor<ChannelSigner>,
        _update_id: chainmonitor::MonitorUpdateId,
    ) -> Result<(), chain::ChannelMonitorUpdateErr> {
        let filename = format!("{}_{}", funding_txo.txid.to_hex(), funding_txo.index);
        util::write_to_file(self.path_to_monitor_data(), filename.clone(), monitor)
            .map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)?;
        if let Some(path) = self.path_to_monitor_data_backup() {
            util::write_to_file(path, filename, monitor)
                .map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)?;
        }
        Ok(())
    }

    fn update_persisted_channel(
        &self,
        funding_txo: OutPoint,
        _update: &Option<ChannelMonitorUpdate>,
        monitor: &ChannelMonitor<ChannelSigner>,
        _update_id: chainmonitor::MonitorUpdateId,
    ) -> Result<(), chain::ChannelMonitorUpdateErr> {
        let filename = format!("{}_{}", funding_txo.txid.to_hex(), funding_txo.index);
        util::write_to_file(self.path_to_monitor_data(), filename.clone(), monitor)
            .map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)?;
        if let Some(path) = self.path_to_monitor_data_backup() {
            util::write_to_file(path, filename, monitor)
                .map_err(|_| chain::ChannelMonitorUpdateErr::PermanentFailure)?;
        }
        Ok(())
    }
}
