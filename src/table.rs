// Records are generated from the mapping data at build time. No runtime parsing
// or initialization, unsafe casts, alignment or platform endianness assumptions.
#[derive(Clone, Copy)]
pub(crate) struct Table(&'static [u8]);

impl Table {
    pub(crate) fn get(self, first: u32, second: u32) -> Option<(u32, u32)> {
        let key = (u64::from(first) << 21) | u64::from(second);
        let mut low = 0;
        let mut high = self.0.len() / 16;
        while low < high {
            let mid = low + (high - low) / 2;
            let row = &self.0[mid * 16..mid * 16 + 16];
            let current = u64::from_le_bytes(row[..8].try_into().unwrap());
            match current.cmp(&key) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    return Some((
                        u32::from_le_bytes(row[8..12].try_into().unwrap()),
                        u32::from_le_bytes(row[12..16].try_into().unwrap()),
                    ));
                }
            }
        }
        None
    }
}

include!(concat!(env!("OUT_DIR"), "/tables.rs"));
