//! Watcher for FS changes and updates the corpus.

use notify::{DebouncedEvent, RecursiveMode, Watcher};
use std::error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvError, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tantivy::schema::{Schema, STORED, STRING, TEXT};
use tantivy::{Document, Index, TantivyError, Term};

pub static FIELD_ID: &str = "file_id";
pub static FIELD_PATH: &str = "path";
pub static FIELD_EXT: &str = "ext";
pub static FIELD_FILENAME: &str = "filename";

pub(crate) struct Indexer<'a> {
    index: Index,
    schema: Schema,
    paths: &'a [&'a Path],
}

pub fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    // The path is the ID for the document, type STRING will ensure it is not tokenized.
    schema_builder.add_text_field(FIELD_ID, STRING);
    // We also tokenize the path and store it, so that we can report it in the results.
    schema_builder.add_text_field(FIELD_PATH, TEXT | STORED);
    // Whilst extension and filename are part of the path, we're also adding them here.
    schema_builder.add_text_field(FIELD_EXT, TEXT);
    schema_builder.add_text_field(FIELD_FILENAME, TEXT);

    schema_builder.build()
}

impl<'a> Indexer<'a> {
    pub fn new(
        index: Index,
        schema: Schema,
        paths: &'a [&'a Path],
    ) -> Result<Self, Box<dyn error::Error>> {
        Ok(Indexer {
            index,
            schema,
            paths,
        })
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

        let mut index_writer = self.index.writer_with_num_threads(1, 50_000_000)?;
        let field_id = self.schema.get_field(FIELD_ID).unwrap();
        let field_path = self.schema.get_field(FIELD_PATH).unwrap();
        let field_ext = self.schema.get_field(FIELD_EXT).unwrap();
        let field_filename = self.schema.get_field(FIELD_FILENAME).unwrap();

        let from_pathbuf = |p: &PathBuf| {
            let mut doc = Document::new();
            doc.add_text(field_id, &p.to_string_lossy());
            doc.add_text(field_path, &p.to_string_lossy());
            match p.extension() {
                Some(s) => doc.add_text(field_ext, &s.to_string_lossy()),
                None => (),
            }
            match p.file_name() {
                Some(s) => doc.add_text(field_filename, &s.to_string_lossy()),
                None => (),
            }
            doc
        };

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
                        index_writer.add_document(from_pathbuf(&p));
                    }
                    Err(e) => {
                        error!("Walkdir Error: {}", e);
                    }
                }
            }
            debug!("Commiting the index.");
            index_writer.commit()?;
            let duration = start.elapsed();
            info!(
                "Indexing complete for: {} in {}s",
                path_str,
                duration.as_secs()
            );
        }

        info!("Indexer watching for change events...");
        // Wait for watcher events and index those.
        let mut counter: u32 = 1;
        let mut last_change = counter;
        loop {
            // This will increment the counter and commit if we have processed
            // a number of documents (1000). This is to prevent us never
            // getting to the commit timeout if we are constantly churning
            // events.
            if counter % 1000 == 0 {
                info!("Commiting index after 1000 mutations.");
                match index_writer.commit() {
                    Ok(_) => (),
                    Err(e) => error!("Could not commit IndexWriter: {}", e),
                };
            }

            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(WatchEvent::Create(pb)) => {
                    debug!("CREATE: {:?}", pb);
                    index_writer.add_document(from_pathbuf(&pb));
                    counter += 1;
                }
                Ok(WatchEvent::Remove(pb)) => {
                    debug!("REMOVE: {:?}", pb);
                    let term = Term::from_field_text(field_id, &pb.to_string_lossy());
                    index_writer.delete_term(term);
                    counter += 1;
                }
                Ok(WatchEvent::Rename(pb_src, pb_dst)) => {
                    debug!("RENAME: {:?} -> {:?}", pb_src, pb_dst);
                    let term = Term::from_field_text(field_id, &pb_src.to_string_lossy());
                    index_writer.delete_term(term);
                    index_writer.add_document(from_pathbuf(&pb_dst));
                    counter += 1;
                }
                Err(e) => match e {
                    RecvTimeoutError::Timeout => {
                        // Don't keep commiting if we're just idle.
                        if last_change != counter {
                            debug!("Commiting index after receiver timeout");
                            last_change = counter;
                            match index_writer.commit() {
                                Ok(_) => (),
                                Err(e) => error!("Could not commit IndexWriter: {}", e),
                            }
                        }
                    }
                    RecvTimeoutError::Disconnected => {
                        error!("Error from the RX channel for the FsWatcher: {}", e);
                        return Err(IndexerError::WatcherRxError(e));
                    }
                },
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
    Tantivy(TantivyError),
    WatcherRxError(RecvTimeoutError),
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

impl From<io::Error> for IndexerError {
    fn from(e: io::Error) -> Self {
        IndexerError::IoError(e)
    }
}

impl From<TantivyError> for IndexerError {
    fn from(e: TantivyError) -> Self {
        IndexerError::Tantivy(e)
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

    #[test]
    fn test_tantivy() {
        use tantivy::collector::TopDocs;
        use tantivy::query::QueryParser;
        use tantivy::schema::*;
        use tantivy::{doc, Index};

        let mut schema_builder = Schema::builder();
        let title = schema_builder.add_text_field("doc_id", STRING | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema.clone());

        let title_str = "/foo/bar/baz";

        let mut index_writer = index.writer_with_num_threads(1, 50_000_000).unwrap();
        index_writer.add_document(doc!(title => title_str));
        index_writer.commit().unwrap();

        let searcher = index.reader().unwrap().searcher();
        let query_parser = QueryParser::for_index(&index, vec![title]);
        let query_promo = query_parser.parse_query(title_str).unwrap();
        let top_docs_promo = searcher
            .search(&query_promo, &TopDocs::with_limit(1))
            .unwrap();

        assert_eq!(top_docs_promo.len(), 1);

        let term = Term::from_field_text(title, title_str);
        index_writer.delete_term(term);
        index_writer.commit().unwrap();

        let reader = index.reader().unwrap();
        reader.reload().unwrap();
        let searcher2 = reader.searcher();
        let query_promo2 = query_parser.parse_query(title_str).unwrap();
        let top_docs_promo2 = searcher2
            .search(&query_promo2, &TopDocs::with_limit(1))
            .unwrap();

        assert_eq!(top_docs_promo2.len(), 0);
    }
}
