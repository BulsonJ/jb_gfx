use anyhow::Result;

struct AssetFile {
    file_type: [char;4],
    version: i32,
    json: String,
    binary_blob: Vec<char>
}

impl AssetFile {
    pub fn save_binary_file(&self, path: &str) -> Result<()> {
        todo!()
    }

    pub fn load_binary_file(path: &str) -> Result<AssetFile> {
        todo!()
    }
}

enum TextureFormat {
    Unknown,
    RGBA8,
}

struct TextureInfo {
    size: u64,
    format: TextureFormat,
    pixel_size: [u32; 3],
    original_file: String,
}

impl TextureInfo {
    pub fn read_texture_info(asset_file: AssetFile) -> TextureInfo {
        todo!()
    }

    pub fn unpack_texture(&self) {
        todo!()
    }

    pub fn pack_texture(&self) -> AssetFile {
        todo!()
    }
}