use std::collections::{HashMap, HashSet};
use std::error;
use std::fmt;
use std::path::{Component, PathBuf};

#[derive(Debug)]
pub struct IndexError;

impl error::Error for IndexError {}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IndexError: {:#?}", self)?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Index {
    data: HashMap<String, HashSet<String>>,
}

impl Index {
    pub fn new() -> Self {
        // TODO: Don't be in-memory...
        // This should build an inverted index of the entries
        Index {
            data: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, entry: IndexItem) -> Result<(), IndexError> {
        for k in &entry.keys {
            self.data
                .entry(k.clone())
                .or_insert(HashSet::new())
                .insert(entry.value.clone());
        }

        Ok(())
    }

    pub fn remove(&mut self, entry: IndexItem) -> Result<(), IndexError> {
        for k in &entry.keys {
            if let Some(v) = self.data.get_mut(k) {
                v.remove(&entry.value);
                if v.is_empty() {
                    self.data.remove(k);
                }
            };
        }

        Ok(())
    }

    pub fn query(&self, q: &str) -> Result<Vec<String>, IndexError> {
        let mut r = HashSet::new();

        for k in self.data.keys() {
            if k.contains(q) {
                let rs = self.data.get(k).unwrap();
                for v in rs {
                    // TODO: FInd a better way than cloning out the strings...
                    r.insert(v.clone());
                }
            }
        }

        let mut ret = Vec::with_capacity(r.len());
        for v in r {
            ret.push(v);
        }

        Ok(ret)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct IndexItem {
    keys: Vec<String>,
    value: String,
}

impl From<PathBuf> for IndexItem {
    fn from(pb: PathBuf) -> Self {
        let mut keys = Vec::new();
        for c in pb.components() {
            match c {
                Component::Normal(p) => keys.push(String::from(p.to_string_lossy())),
                _ => (),
            }
        }

        IndexItem {
            keys,
            value: String::from(pb.to_string_lossy()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_index() {
        let mut idx = Index::new();

        let pb1 = PathBuf::from("/foo/bar/baz_1");
        let pb2 = PathBuf::from("/foo/bar/baz_2");
        let pb3 = PathBuf::from("/a/b/c/d/e");
        let pb4 = PathBuf::from("/fooo/bar/aaaaa");
        let pb5 = PathBuf::from("/1/2/3/4/5/aaa.b.foo");

        // println!("Index: {:#?}", idx);

        idx.insert(pb1.into()).unwrap();
        idx.insert(pb2.into()).unwrap();
        idx.insert(pb3.into()).unwrap();
        idx.insert(pb4.into()).unwrap();
        idx.insert(pb5.into()).unwrap();

        // println!("Index: {:#?}", idx);

        assert_eq!(4, idx.query("foo").unwrap().len());
        assert_eq!(0, idx.query("ABABA").unwrap().len());
        assert_eq!(3, idx.query("bar").unwrap().len());
        assert_eq!(3, idx.query("ba").unwrap().len());

        idx.remove(PathBuf::from("/foo/bar/baz_1").into()).unwrap();

        assert_eq!(3, idx.query("foo").unwrap().len());
    }
}
