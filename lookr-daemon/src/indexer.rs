//! Watcher for FS changes and updates the corpus.

use crate::index::{Index, IndexError};
use notify::{DebouncedEvent, RecursiveMode, Watcher};
use std::error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct Indexer<'a> {
    index: Arc<Mutex<Index>>,
    paths: &'a [&'a Path],
}

impl<'a> Indexer<'a> {
    pub fn new(
        index: Arc<Mutex<Index>>,
        // data_dir: &'a Path,
        paths: &'a [&'a Path],
    ) -> Result<Self, Box<dyn error::Error>> {
        Ok(Indexer { index, paths })
    }

    /// Build the index for the given locations.
    pub fn index(&mut self) -> Result<(), IndexerError> {
        let (tx, rx) = channel();

        info!("Starting FsWatcher thread");
        let w = FsWatcher::new(tx, self.paths)?;
        thread::spawn(move || {
            // This should not return.
            match w.watch() {
                Ok(_) => (),
                Err(e) => error!("Error on watcher thread: {}", e),
            }
        });

        // index all of the items that exist.
        for path in self.paths {
            let start = Instant::now();
            let path_str = path.to_string_lossy();
            info!("Starting index of: {}", path_str);

            let walker = walkdir::WalkDir::new(path);
            for entry in walker {
                match entry {
                    Ok(e) => {
                        let p = e.into_path();
                        debug!("Indexing: {:?}", p);
                        let mut idx = self.index.lock().unwrap();
                        idx.insert(p.into())?;
                    }
                    Err(e) => {
                        error!("Walkdir Error: {}", e);
                    }
                }
            }
            let duration = start.elapsed();
            info!(
                "Indexing complete for: {} in {}s",
                path_str,
                duration.as_secs()
            );
        }

        info!("Indexer watching for change events...");
        // Wait for watcher events and index those.
        loop {
            match rx.recv() {
                Ok(WatchEvent::Create(pb)) => {
                    debug!("CREATE: {:?}", pb);
                    let mut idx = self.index.lock().unwrap();
                    idx.insert(pb.into())?;
                }
                Ok(WatchEvent::Remove(pb)) => {
                    debug!("REMOVE: {:?}", pb);
                    let mut idx = self.index.lock().unwrap();
                    idx.remove(pb.into())?;
                }
                Ok(WatchEvent::Rename(pb_src, pb_dst)) => {
                    debug!("RENAME: {:?} -> {:?}", pb_src, pb_dst);
                    let mut idx = self.index.lock().unwrap();
                    idx.remove(pb_src.into())?;
                    idx.insert(pb_dst.into())?;
                }
                Err(e) => {
                    error!("Error from the RX channel for the FsWatcher: {}", e);
                    return Err(IndexerError::WatcherRxError(e));
                }
            }
        }
    }
}

impl Drop for Indexer<'_> {
    fn drop(&mut self) {
        // Close off open files and end watcher.
    }
}

#[derive(Debug)]
pub enum IndexerError {
    IoError(io::Error),
    Index(IndexError),
    WatcherRxError(RecvError),
    Watcher(WatcherError),
}

impl error::Error for IndexerError {}

impl fmt::Display for IndexerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IndexerError: {:#?}", self)?;
        Ok(())
    }
}

impl From<WatcherError> for IndexerError {
    fn from(e: WatcherError) -> Self {
        IndexerError::Watcher(e)
    }
}

impl From<IndexError> for IndexerError {
    fn from(e: IndexError) -> Self {
        IndexerError::Index(e)
    }
}

impl From<io::Error> for IndexerError {
    fn from(e: io::Error) -> Self {
        IndexerError::IoError(e)
    }
}

#[derive(Debug)]
pub enum WatcherError {
    PathIsNotADir,
    PathDoesNotExist,
    NotifyError(RecvError),
}

impl error::Error for WatcherError {}

impl fmt::Display for WatcherError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "WatcherError: {:#?}", self)?;
        Ok(())
    }
}

#[derive(Debug)]
enum WatchEvent {
    Create(PathBuf),
    Remove(PathBuf),
    Rename(PathBuf, PathBuf),
}

/// Recursively watch on the paths specified, updating the sorpus when they
/// change.
#[derive(Debug)]
struct FsWatcher {
    tx: Sender<WatchEvent>,
    paths: Vec<PathBuf>,
}

impl<'a> FsWatcher {
    fn new(tx: Sender<WatchEvent>, paths: &[&Path]) -> Result<Self, WatcherError> {
        let mut ps = Vec::with_capacity(paths.len());
        for p in paths {
            let p = PathBuf::from(p);
            if !p.exists() {
                return Err(WatcherError::PathDoesNotExist);
            }
            if !p.is_dir() {
                return Err(WatcherError::PathIsNotADir);
            }
            ps.push(p);
        }

        Ok(FsWatcher { tx, paths: ps })
    }

    /// This function will block until termination or an error occurs (which
    /// will be returned in the Result).
    fn watch(&self) -> Result<(), Box<dyn error::Error>> {
        let (tx, rx) = channel();

        let mut watcher = notify::watcher(tx, Duration::from_secs(1))?;

        for path in &self.paths {
            match watcher.watch(path, RecursiveMode::Recursive) {
                Err(e) => error!(
                    "Error attempting to watch {:?}, this path will not be watched for updates: {}",
                    path, e
                ),
                _ => (),
            }
        }

        loop {
            match rx.recv() {
                Ok(DebouncedEvent::Create(pb)) => {
                    self.tx.send(WatchEvent::Create(pb))?;
                }
                Ok(DebouncedEvent::Remove(pb)) => {
                    self.tx.send(WatchEvent::Remove(pb))?;
                }
                Ok(DebouncedEvent::Rename(pb_src, pb_dst)) => {
                    self.tx.send(WatchEvent::Rename(pb_src, pb_dst))?;
                }
                Ok(event) => {
                    debug!("Watcher: Other event: {:?}", event);
                }
                Err(e) => {
                    error!("Error on watcher channel: {}", e);
                    return Err(Box::new(WatcherError::NotifyError(e)));
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_indexer() {
        if true {
            return;
        }; // This was just for testing the indexer interactively.
        let paths = vec![Path::new("src")];

        let idx = Arc::new(Mutex::new(Index::new()));
        let mut i = Indexer::new(idx, &paths).unwrap();

        i.index().unwrap();
    }

    #[test]
    fn test_pb() {
        let pb = PathBuf::from("/foo/bar/baz/some/file.f");
        for p in pb.iter() {
            println!("{:?}", p);
        }
        println!("{:?}", pb.extension());
        println!("{:?}", pb.file_name());

        // let pb2 = pb.canonicalize().unwrap();

        for c in pb.components() {
            match c {
                std::path::Component::Normal(n) => {
                    println!("NORMAL: {:?}", n);
                }
                c => println!("OTHER: {:?}", c),
            }
        }
    }
}
