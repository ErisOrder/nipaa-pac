use binrw::{
    BinRead, NullString, FilePtr32, BinWrite, binwrite, BinWriterExt
};
use clap::Parser;
use std::io::Cursor;
use std::path::Path;
use std::{io::SeekFrom, fs::DirBuilder};
use std::fs::{File, remove_dir_all, read_dir};
use anyhow::{Result, bail, Context};
use miniz_oxide::inflate::decompress_to_vec_zlib;
use miniz_oxide::deflate::compress_to_vec_zlib;
use encoding_rs::SHIFT_JIS;

const ENTRY_NAME_SIZE: usize = 56;

/// Struct for reading archive entries
///
/// Real layout:
/// ```
/// ptr: u32,
/// size: u32,
/// name: [u8; 56]
/// ```
#[derive(BinRead)]
struct PacEntryRead {
    #[br(seek_before = SeekFrom::Current(4))]
    pub size: u32,
    #[br(seek_before = SeekFrom::Current(-8), args(size), err_context("size = {size}"))]
    pub file: FilePtr32<PacFile>,
    #[br(seek_before = SeekFrom::Current(4), pad_size_to = ENTRY_NAME_SIZE)]
    pub name: NullString,
}

impl PacEntryRead {
    /// Try to get file name
    pub fn name(&self) -> Result<String> {
        match SHIFT_JIS.decode(&self.name) {
            (cow, _, false) => Ok(cow.to_string()),
            (cow, _, true) => bail!("failed to normally decode string: {cow}")
        }
    }
}

/// Struct for reading Pac archive
#[allow(dead_code)]
#[derive(BinRead)]
struct PacArc {
    pub entries_count: u32,
    #[br(count = entries_count)]
    pub entries: Vec<PacEntryRead>,
}

/// Entry struct for writing to archive
#[binwrite]
#[repr(C)]
struct PacEntryWrite {
    pub offset: u32,
    #[bw(calc = data.size() as u32)]
    pub size: u32,
    #[bw(pad_size_to = ENTRY_NAME_SIZE)]
    pub name: NullString,
    #[bw(ignore)]
    pub data: PacFile,
}

/// Builder for Pac archives
struct PacArcBuilder {
    pub entries: Vec<PacEntryWrite>,
}

impl PacArcBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            entries: vec![],
        }
    }

    /// Add new entry to archive
    pub fn add_entry(&mut self, file: PacFile, name: &str) -> Result<()> {
        let enc_name = match SHIFT_JIS.encode(name) {
            (cow, _, false) if cow.len() < ENTRY_NAME_SIZE => cow.to_vec(),
            (_, _, true) => bail!("Failed to encode entry name: {name}"),
            (cow, _, false) => 
                bail!("Too long entry name ({}): {name} (must not exceed {ENTRY_NAME_SIZE} bytes)", cow.len())
        };
        
        let e = PacEntryWrite {
            name: NullString(enc_name),
            data: file,
            offset: 0,
        };

        self.entries.push(e);

        Ok(())
    }

    /// Pack all entries to archive
    pub fn pack(self, out_path: &str) -> Result<()> {
        let mut out = File::create(out_path)?;

        out.write_le(&(self.entries.len() as u32))?;

        let mut header_buff = Cursor::new(vec![]);
        let mut data_buff = Cursor::new(vec![]);
        
        let mut current_offset = 
            (std::mem::size_of::<PacEntryWrite>() * self.entries.len() + 4) as u32;

        for mut entry in self.entries {
            entry.offset = current_offset;
            current_offset += entry.data.size() as u32;
            header_buff.write_le(&entry)?;
            data_buff.write_le(&entry.data)?;
        }

        out.write_le(&header_buff.into_inner())?;
        out.write_le(&data_buff.into_inner())?;
                
        Ok(())
    }
}

impl PacArc {
    /// Extract and convert all files
    pub fn extract_all(&self, out_dir: &str) -> Result<()> {
        for entry in self.entries.iter() {
            let name = entry.name()?;
            // Replace file name and extension
            let path = Path::new(&format!("{out_dir}/x"))
                .with_file_name(&name)
                .with_extension(PacFile::converted_ext(
                    Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("") 
                ));

            std::fs::write(path, entry.file.converted_data().context("Failed to extract {path}")?)?;
        }   
        Ok(()) 
    } 
}

/// Representation of files found in archive
#[derive(BinRead, BinWrite)]
#[br(import(size: u32))]
enum PacFile {
    /// BMP file compressed with zlib 
    #[brw(magic = b"ZLC3")]
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

    /// Get original (packed) extension
    pub fn original_ext(conv_ext: &str) -> &str {
        match conv_ext {
            "bmp" => "bmz",
            other => other,
        }
    }

    /// Get converted (extracted) extension
    pub fn converted_ext(orig_ext: &str) -> &str {
        match orig_ext {
            "bmz" => "bmp",
            other => other,
        }
    }


    /// Try to build file from raw data.
    /// Expects extension of converted file
    pub fn convert_back(data: Vec<u8>, conv_extension: &str) -> Result<Self> {
        match conv_extension {
            "bmp" => {
                let uncompressed_size = data.len() as u32;
                let compressed_data = compress_to_vec_zlib(&data, 5);
                Ok(PacFile::Bmz { uncompressed_size, compressed_data })                
            } 
            _ => Ok(PacFile::Other { data })
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
    /// List all files in archive
    #[clap(visible_alias = "l")]
    List {
        /// .pac archive
        arc: String,
    },
    /// Pack directory into archive
    #[clap(visible_alias = "p")]
    Pack {
        /// Result will be saved to this file
        out_arc: String,
        /// Build archive from this directory
        src_dir: String,
    }
}

fn main() -> Result<()> {
    let args = Commands::parse();

    match args {
        Commands::Extract { arc, out_dir } => {
            let mut f = File::open(arc)?;
            let arc = PacArc::read_le(&mut f)?;

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
            let arc = PacArc::read_le(&mut f)?;

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

                println!("{idx:<6}{:<10}{info:<48}{name}", entry.size);
            }
        },
        Commands::Pack { out_arc, src_dir } => {
            let mut builder = PacArcBuilder::new();
            
            for entry in read_dir(src_dir)? {
                let entry = entry?;
                if entry.metadata()?.is_file() {
                    let unc_data = std::fs::read(entry.path())?;
                    let path = entry.path();

                    let unc_ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();
                    
                    let pac_file = PacFile::convert_back(unc_data, unc_ext)?;

                    let path = path.with_extension(PacFile::original_ext(unc_ext));
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap();

                    builder.add_entry(pac_file, name)?;                    
                }    
                else {
                    bail!("all source directory entries must be files")
                }
            }

            builder.pack(&out_arc)?;
            println!("All files packed")
        },
    }

    Ok(())
}
