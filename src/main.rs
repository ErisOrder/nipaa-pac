use binrw::{
    BinRead, NullString, FilePtr32,
};
use clap::Parser;
use std::path::Path;
use std::{io::SeekFrom, fs::DirBuilder};
use std::fs::{File, remove_dir_all};
use anyhow::{Result, bail};
use miniz_oxide::inflate::decompress_to_vec_zlib;

use encoding_rs::SHIFT_JIS;

/// Real layout:
/// ```
/// ptr: u32,
/// size: u32,
/// name: [u8; 56]
/// ```
#[derive(BinRead)]
#[br(little)]
struct PacEntry {
    #[br(seek_before = SeekFrom::Current(4))]
    pub size: u32,
    #[br(seek_before = SeekFrom::Current(-8), args(size), err_context("size = {size}"))]
    pub file: FilePtr32<PacFile>,
    #[br(seek_before = SeekFrom::Current(4), pad_size_to = 56)]
    pub name: NullString,
}


impl PacEntry {
    /// Try to get file name
    pub fn name(&self) -> Result<String> {
        match SHIFT_JIS.decode(&self.name) {
            (cow, _, false) => Ok(cow.to_string()),
            (cow, _, true) => bail!("failed to normally decode string: {cow}")
        }
    }

    /// Get converted data
    pub fn converted_data(&self) -> Result<Vec<u8>> {
        if let Some(data) = &self.file.value {
            data.converted_data()
        } else {
            bail!("no file data")
        }
    } 
}

#[derive(BinRead)]
#[br(little)]
struct PacArc {
    pub entries_count: u32,
    #[br(count = entries_count)]
    pub entries: Vec<PacEntry>,
    
}

impl PacArc {
    pub fn extract_all(&self, out_dir: &str) -> Result<()> {
        for entry in self.entries.iter() {
            let path = format!("{out_dir}/{}", entry.name().unwrap());
            std::fs::write(path, entry.converted_data().expect("failed to get data"))?;
        }   
        Ok(()) 
    } 
}

#[derive(BinRead)]
#[br(import(size: u32))]
enum PacFile {
    #[br(magic = b"ZLC3")] 
    Bmz {
        uncompressed_size: u32, 
        #[br(count = size - Self::BMZ_HEADER_SIZE as u32)]
        compressed_data: Vec<u8>,
    },

    Other {
        #[br(count = size, err_context("size = {}", size))]
        data: Vec<u8>
    }
}

impl PacFile {
    const BMZ_HEADER_SIZE: usize = 8;

    /// Get converted data
    pub fn converted_data(&self) -> Result<Vec<u8>> {
        match self {
            PacFile::Bmz { compressed_data, .. } => {
                match decompress_to_vec_zlib(compressed_data) {
                    Ok(data) => Ok(data),
                    Err(e) => bail!(e),
                }
            },
            PacFile::Other { data } => Ok(data.clone()),
        }
    }

    // Get file size
    pub fn size(&self) -> usize {
        match self {
            PacFile::Bmz { compressed_data, .. } => compressed_data.len() + Self::BMZ_HEADER_SIZE,
            PacFile::Other { data } => data.len(),
        }
    }
}

/// Utility for extracting and packing pac archives of ひぐらしのなく頃に礼　デスクトップアクセサリー
/// (higurashi no naku koro ni screen buddy)
#[derive(Parser)]
enum Commands {
    /// Extract all files from `arc` to `out_dir`
    #[clap(visible_alias = "x")]
    Extract {
        /// .pac archive
        arc: String,
        /// out folder, will be created if not exists, all contents will be REMOVED if exists
        out_dir: String,
    },
    #[clap(visible_alias = "l")]
    /// List all files in archive
    List {
        /// .pac archive
        arc: String,
    }
    // Pack,
}

fn main() -> Result<()> {
    let args = Commands::parse();

    match args {
        Commands::Extract { arc, out_dir } => {
            let mut f = File::open(arc)?;
            let arc = PacArc::read(&mut f)?;

            let path = Path::new(&out_dir);
            match (path.exists(), path.is_dir()) {
                (true, true) => remove_dir_all(path)?,
                (true, false) => bail!("specified path is not a directory"),
                _ => (),
            }

            DirBuilder::new().create(path)?;
            arc.extract_all(&out_dir)?;
            println!("All files extracted successfully");
        },
        Commands::List { arc } => {
            let mut f = File::open(arc)?;
            let arc = PacArc::read(&mut f)?;

            println!("{:<6}{:<10}{:<48}{}", "index", "size", "info", "name");
            for (idx, entry) in arc.entries.iter().enumerate() {
                let info = match &*entry.file {
                    PacFile::Bmz { uncompressed_size, .. } =>
                        format!("bmz uncompressed size: {uncompressed_size}"),
                    PacFile::Other { .. } =>  "other file".into(),
                };

                let name = match entry.name() {
                    Ok(n) => n,
                    Err(e) => e.to_string(),
                };

                println!("{idx:<6}{:<10}{info:<48}{name}", entry.file.size(), );
            }
        },
    }

    Ok(())
}
