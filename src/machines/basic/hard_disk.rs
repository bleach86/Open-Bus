const BLOCK: usize = 512;

pub struct Volume {
    pub blocks: Vec<u8>,
    pub path: PathBuf,
    offset: u64,
    pub write_protected: bool,
}
struct HardDisk {}
