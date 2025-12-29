use color_eyre::eyre::{eyre, ContextCompat, Result, WrapErr};
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

#[derive(Debug)]
pub struct LibraryFile {
    pub hash: FileHash,
    pub path_in_library: PathBuf
}

#[derive(ValueEnum, Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "confique::serde")] 
pub enum SortPolicy {
    Date,
    MoveToRoot
}

impl Default for SortPolicy {
    fn default() -> Self {
        SortPolicy::MoveToRoot
    }
}
    
#[derive(Debug)]
pub struct Library {
    output_root: PathBuf,
    meta_root: PathBuf,
    files: Vec<LibraryFile>
}

const CONTENT_SENTINEL: &'static str = "--START-CONTENT--";
const SUPPORTED_VERSION_MAX: u16 = 1;
const CURRENT_VERSION: u16 = 1;
const HASH_LENGTH: u8 = 64;

impl Library {
    pub fn persist_to_disk(self) -> Result<()> {
        let meta_root = self.meta_root;
        {
            let mut hash_path = meta_root.clone();
            hash_path.push("hashes");

            assert!(hash_path.exists(), "hash path should exist");

            let hash_content = self.files.into_iter()
                .fold(String::new(), |mut a, b| {
                    a.push_str(&b.hash.encode());
                    a.push_str(" ");
                    a.push_str(&b.path_in_library.to_string_lossy());
                    a.push_str("\n");
                    a
                });
            fs::write(
                hash_path,
                format!(
                    "{}\n{}\n{}",
                    CURRENT_VERSION.to_string(),
                    CONTENT_SENTINEL.to_string(),
                    hash_content
                )
            )
        }?;
        Ok(())
    }

    fn ensure_meta_file(&self, file_name: &'static str) -> Result<(PathBuf, bool)> {
        let path = self.meta_root.join(file_name);

        if !path.exists() {
            fs::File::create(&path)
                .wrap_err(format!("when creating meta file {} ({})", file_name, path.display()))?;
            Ok((path, true))
        } else {
            Ok((path, false))    
        }
    }

    fn read_hash_file(&self) -> Result<Vec<LibraryFile>> {
        let (hash_path, file_created) = self.ensure_meta_file("hashes")?;
        if file_created {
            return Ok(vec![])
        }

        let content = fs::read_to_string(hash_path)?;
        let (version, hashes) = content
            .split_once(CONTENT_SENTINEL)
            .wrap_err("could not find content sentinel, likely library corruption")?;

        let version = version
            .trim()
            .parse::<u16>()
            .wrap_err("could not parse version information, likely library corruption")?;
        
        if version > SUPPORTED_VERSION_MAX {
            return Err(eyre!("version {version} is not supported. max supported version is {SUPPORTED_VERSION_MAX}"));
        }

        hashes
            .trim()
            .lines()
            .map(|l| {
                let (hash_raw, path) = l.split_at(HASH_LENGTH.into());
                Ok(LibraryFile {
                    hash: FileHash::decode(hash_raw.trim())?,
                    path_in_library: path.trim().into()
                })
            })
            .collect::<Result<Vec<LibraryFile>>>()
            .wrap_err("when parsing file hashes from hash file")
    }

    pub fn read_from_disk(output_root: PathBuf) -> Result<Library> {
        let meta_root = output_root.join("_pometa");
        let mut s = Self {
            files: vec![],
            output_root,
            meta_root
        };

        s.files = s.read_hash_file()?;
        
        Ok(s)
    }

    #[instrument(skip_all)]
    pub fn process_inputs(&mut self, inputs: &[PathBuf]) -> Result<Vec<UnsortedFile>> {
        let mut new_files = vec![];
        
        for path in inputs {
            let hash = FileHash::from_file(path)?;
            if self.files.iter().find(|f| f.hash == hash).is_some() {
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
            match sort_policy {
                SortPolicy::MoveToRoot => {
                    let fname = file.path.file_name().expect("path to be a normal file");
                    let mut output = self.output_root.clone();
                    output.push(fname);
                    
                    info!("sorting {} into {}", file.path.display(), output.display());
                    fs::rename(&file.path, output)?;
                    
                    self.files.push(LibraryFile {
                        hash: file.hash,
                        path_in_library: fname.into()
                    })
                },
                SortPolicy::Date => {
                    let meta = file.path.metadata()?;
                    
                    let created = meta.created()?
                        .duration_since(std::time::UNIX_EPOCH)?;
                    let epoch = time::macros::datetime!(1970-01-01 0:00);
                    let created_dt = epoch + created;

                    let mut in_lib = {
                        let mut p = PathBuf::new();
                        p.push(created_dt.year().to_string());
                        p.push((created_dt.month() as u8).to_string());
                        p.push(created_dt.day().to_string());
                        p
                    };

                    dbg!(&in_lib);

                    // Do this before adding fname so we only try and make the dirs
                    fs::create_dir_all(&self.output_root.join(&in_lib))?;

                    let fname = file.path.file_name().expect("path to be a normal file");
                    in_lib.push(fname);
                    let output = self.output_root.join(&in_lib);
                    
                    info!("sorting {} into {}", file.path.display(), output.display());
                    fs::rename(file.path, output)?;

                    dbg!(&in_lib);
                    
                    self.files.push(LibraryFile {
                        hash: file.hash,
                        path_in_library: in_lib
                    })
                }
            }
        }

        Ok(())
    }

    pub fn files(&self) -> &Vec<LibraryFile> {
        &self.files
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
        if value.len() != HASH_LENGTH.into() {
            return Err(eyre!("value was not {HASH_LENGTH} chars long. got {}", value.len()));
        }
        
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
