use serde::{Deserialize, Serialize, de::Visitor};
use binrw::{BinRead, BinWrite};

use encoding_rs::SHIFT_JIS;

/// Encoded animation
#[derive(Serialize, Deserialize, BinRead, BinWrite)]
pub struct TtpFile {
    pub maybe_ttp_type: u32,
    pub frame_count: u32,
    pub window_width: u32,
    pub window_height: u32,
    #[br(count = frame_count)]
    pub frames: Vec<TtpFrame>,
    #[br(if(maybe_ttp_type == 3))]
    pub unk_bool: Option<u8>,
}

/// Frame of animation
#[derive(Serialize, Deserialize, BinRead, BinWrite)]
pub struct TtpFrame {
    pub sprite_name: ResName,
    pub se_name: ResName,
    pub textbox_name: ResName,
    
    pub delay_ms: u32,
    pub x_offset_textbox: u32,
    pub y_offset_textbox: u32,
    pub x_offset: u32,
    pub y_offset: u32,
}

/// Variable-length SHIFT-JIS-encoded resource name
#[derive(BinRead, BinWrite)]
pub struct ResName {
    len: u32,
    #[br(count = len)]
    sj_bytes: Vec<u8>
}

impl Serialize for ResName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        use serde::ser::Error;
        let decoded = match SHIFT_JIS.decode(&self.sj_bytes) {
            (cow, _, false) => cow,
            (_, _, true) => return Err(Error::custom("failed to decode shift-jis")), 
        };
        serializer.serialize_str(&decoded)
    }
}

struct ResNameVisitor;
impl<'de> Visitor<'de> for ResNameVisitor {
    type Value = ResName;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "expected utf-8 encoded string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error, {
        let encoded = match SHIFT_JIS.encode(v) {
            (cow, _, false) => cow,
            (_, _, true) => return Err(E::custom("failed to encode shift-jis"))
        };
        
        Ok(Self::Value {
            len: encoded.len() as u32,
            sj_bytes: encoded.to_vec()
        })
    }
}

impl<'de> Deserialize<'de> for ResName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        deserializer.deserialize_str(ResNameVisitor)
    }
}