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
        if !valid_heap_size(&self.max_heap) {
            return Err(JvmError::InvalidOption(self.max_heap.clone()));
        }
        let class_path = std::env::join_paths(&self.class_path)
            .map_err(|e| JvmError::InvalidOption(e.to_string()))?;
        let class_path = class_path
            .into_string()
            .map_err(|p| JvmError::InvalidOption(p.to_string_lossy().into_owned()))?;
        let mut out = vec![
            format!("-Djava.class.path={class_path}"),
            "--enable-native-access=ALL-UNNAMED".into(),
            "-Xrs".into(),
            "-XX:+UseZGC".into(),
            "-XX:+UseCompactObjectHeaders".into(),
            "-XX:-UsePerfData".into(),
            format!("-Xmx{}", self.max_heap),
        ];
        out.extend(self.extra.iter().cloned());
        if let Some(bad) = out.iter().find(|o| o.contains('\0')) {
            return Err(JvmError::InvalidOption(bad.clone()));
        }
        Ok(out)
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
                "-XX:+UseCompactObjectHeaders".into(),
                "-XX:-UsePerfData".into(),
                "-Xmx256m".into(),
                "-Dmcr.test=1".into(),
            ]
        );
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
