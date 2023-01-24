use binrw::{
    BinRead, ReadOptions, BinResult, VecArgs, NullString, until_eof, FilePtr32,
};

use std::io::{SeekFrom, Read, Seek};
use std::fs::File;
use std::env::args;
use anyhow::Result;

use encoding_rs::SHIFT_JIS;

#[derive(BinRead)]
#[br(little)]
/// Real layout:
/// ```
/// ptr: u32,
/// size: u32,
/// name: [u8; 56]
/// ```
struct PacEntry {
    #[br(seek_before = SeekFrom::Current(4))]
    pub size: u32,
    #[br(seek_before = SeekFrom::Current(-4), args(size), err_context("size = {size}"))]
    pub ptr: FilePtr32<PacFile>,
    #[br(pad_size_to = 56)]
    pub name: NullString,
}


impl PacEntry {
    pub fn name(&self) -> Option<String> {
        match SHIFT_JIS.decode(&self.name) {
            (cow, _, false) => Some(cow.to_string()),
            (_, _, true) => None
        }
    }

    pub fn data(&self) -> Result<Vec<u8>> {
        // println!("data: {}", self.data.inner.len());
        // Ok(self.data.inner.clone())
        if let Some(data) = &self.ptr.value {
            Ok(data.converted_data())
        } else {
            todo!()
        }
    } 
}

// impl std::fmt::Display for PacEntry {    
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:<8}{}", self.data.len(), self.name().unwrap())
//     }
// }

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
            std::fs::write(path, entry.data().expect("failed to get data"))?;
        }   
        Ok(()) 
    } 
}

#[derive(BinRead)]
#[br(import(size: u32))]
enum PacFile {
    #[br(magic = b"ZLC38")] 
    Bmz {
        file_type_or_id_or_layer: u16,
        #[br(magic = b"\x00\x78\xDA")]
        magic2: [u8; 3],
        #[br(count = size - 5)]
        compressed_data: Vec<u8>,
    },

    Other {
        #[br(count = size, err_context("size = {}", size))]
        data: Vec<u8>
    }
}

impl PacFile {
    pub fn converted_data(&self) -> Vec<u8> {
        match self {
            PacFile::Bmz { compressed_data, .. } => compressed_data.clone(),
            PacFile::Other { data } => data.clone(),
        }
    }
}

// #[derive(BinRead)]
// #[br(magic = b"ZLC38")]
// struct BMZFile {
//     file_type_or_id_or_layer: u16,
//     #[br(magic = b"\x00\x78\xDA")]
//     magic2: [u8; 3],
//     #[br(parse_with = until_eof)]
//     compressed_data: Vec<u8>,
// }

fn main() {
    let arc_name = args().nth(1).expect("Expected Arc name");
    let arc_out = args().nth(2).expect("Expected out dir");

    let mut f = File::open(arc_name).unwrap();

    let arc = PacArc::read(&mut f).unwrap();

    for (idx, entry) in arc.entries.iter().enumerate() {
        // println!("{idx:<3}: {entry}")
    }

    std::fs::DirBuilder::new().create(&arc_out).unwrap();

    arc.extract_all(&arc_out).expect("Failed to extract");
}
