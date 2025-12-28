use color_eyre::eyre::{Result, WrapErr};
use std::fmt::Debug;
use std::path::PathBuf;
use sha2::{Sha256, Digest};
use std::{io, fs};
use tracing::{debug, info, instrument};
use clap::ValueEnum;
use confique::serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct UnsortedFile {
    pub hash: FileHash,
    pub path: PathBuf
}

#[derive(ValueEnum, Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "confique::serde")] 
pub enum SortPolicy {
    Date,
    None
}

impl Default for SortPolicy {
    fn default() -> Self {
        SortPolicy::None
    }
}
    
#[derive(Debug)]
pub struct Library {
    output_root: PathBuf,
    meta_root: PathBuf,
    hashes: Vec<FileHash>
}

impl Library {
    pub fn persist_to_disk(self) -> Result<()> {
        let meta_root = self.meta_root;
        {
            let mut hash_path = meta_root.clone();
            hash_path.push("hashes");

            assert!(hash_path.exists(), "hash path should exist");
            fs::write(hash_path,
                      self.hashes.into_iter()
                      .map(|h| h.encode())
                      .fold(String::new(), |mut a, b| {
                          a.reserve(b.len() + 1);
                          a.push_str(&b);
                          a.push_str("\n");
                          a
                      })
                      
            )
        }?;
        Ok(())
    }

    pub fn read_from_disk(output_root: PathBuf) -> Result<Library> {
        let meta_root = output_root.join("_pometa");
        let hashes = {
            let mut hash_path = meta_root.clone();
            hash_path.push("hashes");
            
            if !hash_path.exists() {
                fs::File::create(hash_path)?;
                Ok(vec![])
            } else {
                fs::read_to_string(hash_path)?
                    .lines()
                    .map(FileHash::decode)
                    .collect::<Result<Vec<FileHash>>>()
            }
        }?;

        Ok(Self {
            hashes,
            output_root,
            meta_root
        })
    }

    #[instrument(skip_all)]
    pub fn process_inputs(&mut self, inputs: &[PathBuf]) -> Result<Vec<UnsortedFile>> {
        let mut new_files = vec![];
        
        for path in inputs {
            let hash = FileHash::from_file(path)?;
            if self.hashes.contains(&hash) {
                debug!("file already in library: {} ({})", path.display(), hash.encode());
            } else {
                debug!("found new file: {} ({})", path.display(), hash.encode());
                new_files.push(UnsortedFile { hash, path: path.clone() });
            }
        }

        Ok(new_files)
    }

    #[instrument(skip(self, new_files))]
    pub fn sort_files(
        &mut self,
        new_files: Vec<UnsortedFile>,
        sort_policy: SortPolicy
    ) -> Result<()> {
        info!("sorting {} files", new_files.len());
        for file in new_files {
            self.hashes.push(file.hash);
            
            match sort_policy {
                SortPolicy::None => {
                    let mut output = self.output_root.clone();
                    output.push(file.path.file_name().expect("path to be a normal file"));
                    info!("sorting {} into {}", file.path.display(), output.display());
                    fs::rename(file.path, output)?;
                },
                SortPolicy::Date => {
                    let meta = file.path.metadata()?;
                    
                    let created = meta.created()?
                        .duration_since(std::time::UNIX_EPOCH)?;
                    let epoch = time::macros::datetime!(1970-01-01 0:00);
                    let created_dt = epoch + created;

                    let mut output = self.output_root.clone();
                    output.push(created_dt.year().to_string());
                    output.push((created_dt.month() as u8).to_string());
                    output.push(created_dt.day().to_string());

                    fs::create_dir_all(&output)?;
                    
                    output.push(file.path.file_name().expect("path to be a normal file"));
                    info!("sorting {} into {}", file.path.display(), output.display());
                    fs::rename(file.path, output)?;
                }
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct FileHash(Vec<u8>);

impl Debug for FileHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileHash({})", self.encode())
    }
}

impl FileHash {
    pub fn encode(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn decode(value: &str) -> Result<Self> {
        hex::decode(value)
            .map(FileHash)
            .wrap_err("could not decode hex string")
    }

    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let mut hasher = Sha256::new();
        let mut file = fs::File::open(path)?;
        
        io::copy(&mut file, &mut hasher)?;
        let hash_bytes = hasher.finalize();
        
        Ok(Self(hash_bytes.to_vec()))
    }
}
