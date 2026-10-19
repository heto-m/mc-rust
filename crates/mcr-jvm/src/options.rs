use std::path::PathBuf;

use crate::JvmError;

#[derive(Debug, Clone)]
pub struct JvmOptions {
    pub class_path: Vec<PathBuf>,
    pub max_heap: String,
    pub extra: Vec<String>,
}

impl Default for JvmOptions {
    fn default() -> Self {
        Self {
            class_path: Vec::new(),
            max_heap: "2G".into(),
            extra: Vec::new(),
        }
    }
}

impl JvmOptions {
    pub fn to_strings(&self) -> Result<Vec<String>, JvmError> {
        self.with_dev_options(std::env::var("MCR_JVM_EXTRA").as_deref().unwrap_or(""))
    }

    /// The options with `dev` (whitespace-separated) appended; a collector in `dev` replaces ZGC.
    fn with_dev_options(&self, dev: &str) -> Result<Vec<String>, JvmError> {
        if !valid_heap_size(&self.max_heap) {
            return Err(JvmError::InvalidOption(self.max_heap.clone()));
        }
        let class_path = std::env::join_paths(&self.class_path)
            .map_err(|e| JvmError::InvalidOption(e.to_string()))?;
        let class_path = class_path
            .into_string()
            .map_err(|p| JvmError::InvalidOption(p.to_string_lossy().into_owned()))?;
        let gc_chosen = dev
            .split_whitespace()
            .any(|o| o.starts_with("-XX:+Use") && o.ends_with("GC"));
        let mut out = vec![
            format!("-Djava.class.path={class_path}"),
            "--enable-native-access=ALL-UNNAMED".into(),
            "-Xrs".into(),
        ];
        if !gc_chosen {
            out.push("-XX:+UseZGC".into());
            // ZGC otherwise keeps the whole -Xmx committed for good once a load touched it.
            if let Some(mib) = heap_mib(&self.max_heap) {
                out.push(format!("-XX:SoftMaxHeapSize={}m", mib * 2 / 3));
                out.push("-XX:ZUncommitDelay=5".into());
            }
        }
        out.extend([
            "-XX:+UseCompactObjectHeaders".into(),
            "-XX:-UsePerfData".into(),
            format!("-Xmx{}", self.max_heap),
        ]);
        out.extend(self.extra.iter().cloned());
        out.extend(dev.split_whitespace().map(str::to_owned));
        if let Some(bad) = out.iter().find(|o| o.contains('\0')) {
            return Err(JvmError::InvalidOption(bad.clone()));
        }
        Ok(out)
    }
}

/// `-Xmx`'s value in MiB.
fn heap_mib(s: &str) -> Option<u64> {
    let digits = s.trim_end_matches(['k', 'K', 'm', 'M', 'g', 'G']);
    let n: u64 = digits.parse().ok()?;
    match &s[digits.len()..] {
        "" => Some(n >> 20),
        "k" | "K" => Some(n >> 10),
        "m" | "M" => Some(n),
        "g" | "G" => Some(n << 10),
        _ => None,
    }
}

fn valid_heap_size(s: &str) -> bool {
    let digits = s.trim_end_matches(['k', 'K', 'm', 'M', 'g', 'G']);
    let unit_len = s.len() - digits.len();
    !digits.is_empty() && unit_len <= 1 && digits.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_options_in_order() {
        let opts = JvmOptions {
            class_path: vec!["/a/boot.jar".into(), "/b/lib.jar".into()],
            max_heap: "256m".into(),
            extra: vec!["-Dmcr.test=1".into()],
        };
        let sep = if cfg!(windows) { ';' } else { ':' };
        assert_eq!(
            opts.to_strings().unwrap(),
            [
                format!("-Djava.class.path=/a/boot.jar{sep}/b/lib.jar"),
                "--enable-native-access=ALL-UNNAMED".into(),
                "-Xrs".into(),
                "-XX:+UseZGC".into(),
                "-XX:SoftMaxHeapSize=170m".into(),
                "-XX:ZUncommitDelay=5".into(),
                "-XX:+UseCompactObjectHeaders".into(),
                "-XX:-UsePerfData".into(),
                "-Xmx256m".into(),
                "-Dmcr.test=1".into(),
            ]
        );
    }

    #[test]
    fn dev_options_append_and_a_collector_replaces_zgc() {
        let opts = JvmOptions::default();
        let plain = opts.with_dev_options("  -Xlog:gc  -Dx=1 ").unwrap();
        assert!(plain.contains(&"-XX:+UseZGC".to_owned()));
        assert!(plain.ends_with(&["-Xlog:gc".to_owned(), "-Dx=1".to_owned()]));
        let g1 = opts.with_dev_options("-XX:+UseG1GC").unwrap();
        assert!(!g1.iter().any(|o| o == "-XX:+UseZGC"));
        assert!(!g1.iter().any(|o| o.starts_with("-XX:SoftMaxHeapSize")));
        assert_eq!(g1.last().map(String::as_str), Some("-XX:+UseG1GC"));
        assert!(opts.with_dev_options("-Dx=\0").is_err());
    }

    #[test]
    fn soft_max_is_two_thirds_of_the_heap() {
        assert_eq!(heap_mib("2304m"), Some(2304));
        assert_eq!(heap_mib("2G"), Some(2048));
        assert_eq!(heap_mib("1048576k"), Some(1024));
        let opts = JvmOptions::default().to_strings().unwrap();
        assert!(opts.contains(&"-XX:SoftMaxHeapSize=1365m".to_owned()));
    }

    #[test]
    fn default_heap_is_2g() {
        let opts = JvmOptions::default().to_strings().unwrap();
        assert!(opts.contains(&"-Xmx2G".to_owned()));
    }

    #[test]
    fn rejects_bad_heap_and_nul() {
        for heap in ["", "2GB", "-1g", "m", "2 G"] {
            let opts = JvmOptions {
                max_heap: heap.into(),
                ..Default::default()
            };
            assert!(opts.to_strings().is_err(), "{heap:?}");
        }
        let opts = JvmOptions {
            extra: vec!["-Dx=\0".into()],
            ..Default::default()
        };
        assert!(opts.to_strings().is_err());
    }
}
