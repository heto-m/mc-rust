use std::ffi::c_void;

use crate::{doorbell, host, upcall};

pub const TABLE_MAGIC: u32 = u32::from_be_bytes(*b"MCRB");
/// Magic of embedder tables built with [`ExtensionTable`]; their `version` is the embedder's own.
pub const EXTENSION_MAGIC: u32 = u32::from_be_bytes(*b"MCRX");
/// Bump on any change to [`ENTRIES`] or [`UPCALLS`]; Java refuses a mismatched table.
pub const TABLE_VERSION: u32 = 1;
pub const TABLE_HEADER_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    /// `Linker.Option.critical(false)`: short, never blocks, never calls back.
    Critical,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryDesc {
    pub name: &'static str,
    /// JVM method descriptor over `I`, `J` and `V`.
    pub descriptor: &'static str,
    pub flavour: Flavour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpcallDesc {
    pub name: &'static str,
    pub descriptor: &'static str,
}

macro_rules! native_entries {
    ($($name:literal $desc:literal $flavour:ident => $f:path;)*) => {
        pub const ENTRIES: &[EntryDesc] = &[
            $(EntryDesc { name: $name, descriptor: $desc, flavour: Flavour::$flavour },)*
        ];
        const ENTRY_COUNT: usize = [$($name),*].len();
        static TABLE: NativeTable = NativeTable {
            magic: TABLE_MAGIC,
            version: TABLE_VERSION,
            size: size_of::<NativeTable>() as u32,
            count: ENTRY_COUNT as u32,
            fns: [$(FnPtr($f as *const c_void)),*],
        };
    };
}

native_entries! {
    "register_upcalls" "(JI)I" Normal => upcall::register_upcalls;
    "host_call" "(IJ)J" Normal => host::host_call;
    "doorbell_wait" "(JIJ)I" Normal => doorbell::doorbell_wait;
    "doorbell_notify" "(J)V" Critical => doorbell::doorbell_notify;
    "nop_critical" "(J)J" Critical => nop_critical;
    "nop_normal" "(J)J" Normal => nop_normal;
}

pub const UPCALLS: &[UpcallDesc] = &[
    UpcallDesc {
        name: "selftest_ping",
        descriptor: "()I",
    },
    UpcallDesc {
        name: "selftest_add",
        descriptor: "(JJ)I",
    },
    UpcallDesc {
        name: "selftest_throw",
        descriptor: "()I",
    },
];

#[repr(transparent)]
#[derive(Debug)]
struct FnPtr(*const c_void);

// SAFETY: the pointers are addresses of immutable `extern "C"` functions.
unsafe impl Sync for FnPtr {}

#[repr(C)]
#[derive(Debug)]
struct NativeTable {
    magic: u32,
    version: u32,
    size: u32,
    count: u32,
    fns: [FnPtr; ENTRY_COUNT],
}

pub fn table_address() -> i64 {
    &raw const TABLE as i64
}

/// An embedder-owned second table with the host header layout; the host table stays host-only.
#[derive(Debug)]
pub struct ExtensionTable {
    words: Box<[u64]>,
    entries: Vec<EntryDesc>,
}

impl ExtensionTable {
    /// Leaked: Java may hold the address until the process exits.
    pub fn leak(version: u32, entries: &[(EntryDesc, *const c_void)]) -> &'static Self {
        let header_words = TABLE_HEADER_SIZE / 8;
        let size = TABLE_HEADER_SIZE + entries.len() * 8;
        let pack = |lo: u32, hi: u32| {
            let mut b = [0u8; 8];
            b[..4].copy_from_slice(&lo.to_ne_bytes());
            b[4..].copy_from_slice(&hi.to_ne_bytes());
            u64::from_ne_bytes(b)
        };
        let mut words = vec![0u64; header_words + entries.len()];
        words[0] = pack(EXTENSION_MAGIC, version);
        words[1] = pack(size as u32, entries.len() as u32);
        for (slot, (_, f)) in words[header_words..].iter_mut().zip(entries) {
            *slot = *f as usize as u64;
        }
        Box::leak(Box::new(Self {
            words: words.into_boxed_slice(),
            entries: entries.iter().map(|(d, _)| *d).collect(),
        }))
    }

    pub fn address(&self) -> i64 {
        self.words.as_ptr() as i64
    }

    pub fn entries(&self) -> &[EntryDesc] {
        &self.entries
    }
}

extern "C" fn nop_critical(x: i64) -> i64 {
    x
}

extern "C" fn nop_normal(x: i64) -> i64 {
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn abi_java() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../java/mcr-boot/src/dev/mcr/boot/Abi.java");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn enum_body<'a>(src: &'a str, name: &str) -> &'a str {
        let start = src
            .find(&format!("enum {name} {{"))
            .unwrap_or_else(|| panic!("enum {name} missing"));
        let body = &src[start..];
        &body[..body.find(';').unwrap()]
    }

    fn quoted(line: &str) -> Vec<&str> {
        line.split('"').skip(1).step_by(2).collect()
    }

    fn int_const(src: &str, name: &str) -> i64 {
        let decl = format!("static final int {name} = ");
        let rest = &src[src.find(&decl).unwrap() + decl.len()..];
        let value = rest[..rest.find(';').unwrap()].trim();
        match value.strip_prefix("0x") {
            Some(hex) => i64::from_str_radix(hex, 16).unwrap(),
            None => value.parse().unwrap(),
        }
    }

    #[test]
    fn java_entry_list_matches_rust() {
        let src = abi_java();
        let java: Vec<(String, String, String)> = enum_body(&src, "Entry")
            .lines()
            .filter(|l| l.contains("(\""))
            .map(|l| {
                let q = quoted(l);
                let flavour = l.rsplit("Flavour.").next().unwrap();
                let flavour = flavour.trim_end_matches([')', ',', ' ']).to_owned();
                (q[0].to_owned(), q[1].to_owned(), flavour)
            })
            .collect();
        let rust: Vec<(String, String, String)> = ENTRIES
            .iter()
            .map(|e| {
                let flavour = match e.flavour {
                    Flavour::Critical => "CRITICAL",
                    Flavour::Normal => "NORMAL",
                };
                (e.name.into(), e.descriptor.into(), flavour.into())
            })
            .collect();
        assert_eq!(java, rust);
    }

    #[test]
    fn java_upcall_list_matches_rust() {
        let src = abi_java();
        let java: Vec<(String, String)> = enum_body(&src, "Upcall")
            .lines()
            .filter(|l| l.contains("(\""))
            .map(|l| {
                let q = quoted(l);
                (q[0].to_owned(), q[1].to_owned())
            })
            .collect();
        let rust: Vec<(String, String)> = UPCALLS
            .iter()
            .map(|u| (u.name.into(), u.descriptor.into()))
            .collect();
        assert_eq!(java, rust);
    }

    #[test]
    fn java_header_constants_match_rust() {
        let src = abi_java();
        assert_eq!(int_const(&src, "MAGIC"), i64::from(TABLE_MAGIC));
        assert_eq!(
            int_const(&src, "EXTENSION_MAGIC"),
            i64::from(EXTENSION_MAGIC)
        );
        assert_eq!(int_const(&src, "VERSION"), i64::from(TABLE_VERSION));
        assert_eq!(
            int_const(&src, "TABLE_HEADER_SIZE"),
            TABLE_HEADER_SIZE as i64
        );
    }

    #[test]
    fn java_ring_constants_match_rust() {
        use crate::ring::*;
        let src = abi_java();
        assert_eq!(int_const(&src, "RING_MAGIC"), i64::from(RING_MAGIC));
        assert_eq!(int_const(&src, "RING_HEADER_SIZE"), RING_HEADER_SIZE as i64);
        assert_eq!(
            int_const(&src, "RING_WRITE_OFFSET"),
            RING_WRITE_OFFSET as i64
        );
        assert_eq!(int_const(&src, "RING_READ_OFFSET"), RING_READ_OFFSET as i64);
        assert_eq!(
            int_const(&src, "RECORD_HEADER_SIZE"),
            RECORD_HEADER_SIZE as i64
        );
        assert_eq!(int_const(&src, "PADDING_KIND"), i64::from(PADDING_KIND));
    }

    #[test]
    fn header_describes_the_table() {
        let t = &TABLE;
        assert_eq!(t.magic, 0x4D43_5242);
        assert_eq!(t.count as usize, ENTRIES.len());
        assert_eq!(
            t.size as usize,
            TABLE_HEADER_SIZE + ENTRIES.len() * size_of::<*const c_void>()
        );
        assert_eq!(std::mem::offset_of!(NativeTable, fns), TABLE_HEADER_SIZE);
        assert!(t.fns.iter().all(|f| !f.0.is_null()));
        assert_eq!(table_address() % 8, 0);
    }

    extern "C" fn ext_double(x: i64) -> i64 {
        x * 2
    }

    #[test]
    fn extension_table_has_the_host_header_layout() {
        let desc = EntryDesc {
            name: "double",
            descriptor: "(J)J",
            flavour: Flavour::Critical,
        };
        let t = ExtensionTable::leak(7, &[(desc, ext_double as *const c_void)]);
        assert_eq!(t.address() % 8, 0);
        assert_eq!(t.entries(), [desc]);
        // SAFETY: the leaked table holds a 16-byte header plus one pointer.
        let (magic, version, size, count, f) = unsafe {
            let p = t.address() as *const u32;
            let f = *(t.address() as *const usize).add(2);
            (*p, *p.add(1), *p.add(2), *p.add(3), f)
        };
        assert_eq!((magic, version, size, count), (EXTENSION_MAGIC, 7, 24, 1));
        // SAFETY: the slot holds `ext_double`, whose signature matches.
        let g: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(f) };
        assert_eq!(g(21), 42);
    }

    #[test]
    fn descriptors_use_supported_types() {
        let all = ENTRIES
            .iter()
            .map(|e| e.descriptor)
            .chain(UPCALLS.iter().map(|u| u.descriptor));
        for d in all {
            let (args, ret) = d[1..].split_once(')').unwrap();
            assert!(d.starts_with('('), "{d}");
            assert!(args.chars().all(|c| c == 'I' || c == 'J'), "{d}");
            assert!(matches!(ret, "I" | "J" | "V"), "{d}");
        }
    }
}
