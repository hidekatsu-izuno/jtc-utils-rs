//! Compile hexadecimal mapping data into immutable, sorted binary tables.

use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut generated = String::new();
    let mut paths: Vec<_> = fs::read_dir("data")
        .expect("mapping directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "csv"))
        .collect();
    paths.sort();
    for path in paths {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let name = stem.to_ascii_uppercase().replace(['.', '-'], "_");
        let mut rows = BTreeMap::new();
        for (index, line) in fs::read_to_string(&path).unwrap().lines().enumerate() {
            let mut columns = line.split(',');
            let from = columns.next().unwrap();
            let to = columns.next().expect("two mapping columns");
            let parse = |value: &str| {
                let values: Vec<_> = value
                    .split('+')
                    .map(|part| {
                        u32::from_str_radix(part, 16).unwrap_or_else(|_| {
                            panic!("{}:{}: invalid hex", path.display(), index + 1)
                        })
                    })
                    .collect();
                assert!(!values.is_empty() && values.len() <= 2);
                (values[0], values.get(1).copied().unwrap_or(0))
            };
            let (first, second) = parse(from);
            let value = parse(to);
            if stem.starts_with("encode.") || stem.starts_with("map.") {
                assert!(char::from_u32(first).is_some());
                assert!(second == 0 || char::from_u32(second).is_some());
            }
            if stem.starts_with("decode.") || stem.starts_with("map.") {
                assert!(char::from_u32(value.0).is_some());
                assert!(value.1 == 0 || char::from_u32(value.1).is_some());
            }
            if stem.starts_with("encode.") {
                assert!(value.0 <= 0xffff && value.1 == 0);
            }
            if stem.starts_with("decode.") {
                assert!(first <= 0xffff && second == 0);
            }
            let key = (u64::from(first) << 21) | u64::from(second);
            assert!(
                rows.insert(key, value).is_none(),
                "duplicate in {}",
                path.display()
            );
        }
        let mut bytes = Vec::with_capacity(rows.len() * 16);
        for (key, (first, second)) in rows {
            bytes.extend_from_slice(&key.to_le_bytes());
            bytes.extend_from_slice(&first.to_le_bytes());
            bytes.extend_from_slice(&second.to_le_bytes());
        }
        fs::write(out.join(format!("{stem}.bin")), bytes).unwrap();
        generated.push_str(&format!(
            "pub(crate) const {name}: Table = Table(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{stem}.bin\")));\n"
        ));
    }
    fs::write(out.join("tables.rs"), generated).unwrap();
}
