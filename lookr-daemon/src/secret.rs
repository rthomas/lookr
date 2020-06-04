//! Manages the user secrets.

use std::error;
use std::io;
use std::path::{Path, PathBuf};

pub struct SecretManager<'a> {
    data_dir: &'a PathBuf,
}

impl<'a> SecretManager<'a> {
    pub fn new(data_dir: &'a PathBuf) -> io::Result<Self> {
        if !data_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{:?} does not exist", data_dir),
            ));
        }
        if !data_dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{:?} is not a directory", data_dir),
            ));
        }

        Ok(SecretManager { data_dir })
    }

    /// Returns the path to the users secret, this will create a secret for the
    /// given user if the user exists.
    pub fn get_path_for_user(user: &str) -> Result<Option<PathBuf>, Box<dyn error::Error>> {
        // Check for an existing secret for the given user, and the user matches.

        // Determine if the user exists.

        // If it exists and there is no secret, then create one and write the secret.

        todo!()
    }
}
