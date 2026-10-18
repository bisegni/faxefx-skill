//! Content-addressed local audio. Only opaque IDs enter app tools.
use rodio::Source;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub name: String,
    pub extension: String,
    pub bytes: u64,
    pub duration: f64,
    pub waveform: Vec<f32>,
    pub rms: f64,
}
pub struct Library {
    root: PathBuf,
}
impl Library {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self { root })
    }
    pub fn import(&self, path: &Path) -> Result<Artifact, String> {
        let meta = path.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > 100 * 1024 * 1024 {
            return Err("Choose an audio file up to 100 MiB".into());
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !["wav", "flac", "aiff", "aif", "mp3", "ogg", "m4a"].contains(&ext.as_str()) {
            return Err("Unsupported audio format".into());
        }
        let mut source = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut data = vec![];
        (&mut source)
            .take(100 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 != meta.len() {
            return Err("Audio changed during import".into());
        }
        let id = format!("{:x}", Sha256::digest(&data));
        let target = self.root.join(format!("{id}.{ext}"));
        if !target.exists() {
            let temp = self.root.join(format!("{}.part", uuid::Uuid::new_v4()));
            fs::write(&temp, &data).map_err(|e| e.to_string())?;
            fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        }
        let decoder = rodio::Decoder::try_from(fs::File::open(&target).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let rate = decoder.sample_rate();
        let channels = decoder.channels();
        let limit = rate as usize * channels as usize * 120;
        let mut peaks = vec![];
        let mut peak = 0_f32;
        let mut sum = 0_f64;
        let mut count = 0_usize;
        let stride = (rate as usize * channels as usize / 100).max(1);
        for sample in decoder.take(limit) {
            if !sample.is_finite() {
                return Err("Audio contains non-finite samples".into());
            }
            sum += (sample as f64).powi(2);
            peak = peak.max(sample.abs());
            count += 1;
            if count.is_multiple_of(stride) {
                peaks.push(peak);
                peak = 0.;
            }
        }
        if count == 0 {
            return Err("Audio contains no samples".into());
        }
        if !count.is_multiple_of(stride) {
            peaks.push(peak)
        }
        let waveform = peaks
            .chunks(peaks.len().div_ceil(256).max(1))
            .map(|p| p.iter().copied().fold(0., f32::max))
            .collect();
        let a = Artifact {
            id,
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(255)
                .collect(),
            extension: ext,
            bytes: meta.len(),
            duration: count as f64 / rate as f64 / channels as f64,
            waveform,
            rms: (sum / count as f64).sqrt(),
        };
        let manifest = self.root.join(format!("{}.json", a.id));
        fs::write(manifest, serde_json::to_vec(&a).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(a)
    }
    pub fn get(&self, id: &str) -> Result<Artifact, String> {
        validate_id(id)?;
        let a: Artifact = serde_json::from_slice(
            &fs::read(self.root.join(format!("{id}.json"))).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if a.id != id {
            return Err("Artifact identity mismatch".into());
        }
        Ok(a)
    }
    pub fn path(&self, id: &str) -> Result<PathBuf, String> {
        let a = self.get(id)?;
        if !["wav", "flac", "aiff", "aif", "mp3", "ogg", "m4a"].contains(&a.extension.as_str()) {
            return Err("Invalid artifact extension".into());
        }
        let p = self.root.join(format!("{id}.{}", a.extension));
        let data = fs::read(&p).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(&data)) != id {
            return Err("Audio artifact changed".into());
        }
        Ok(p)
    }
    pub fn excerpt_rms(&self, id: &str, start: f64, duration: f64) -> Result<f64, String> {
        if !start.is_finite()
            || !(0.0..=86400.).contains(&start)
            || !duration.is_finite()
            || duration <= 0.
            || duration > 120.
        {
            return Err("Invalid audio excerpt".into());
        }
        let source =
            rodio::Decoder::try_from(fs::File::open(self.path(id)?).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let mut sum = 0_f64;
        let mut n = 0_u64;
        for sample in source
            .skip_duration(std::time::Duration::from_secs_f64(start))
            .take_duration(std::time::Duration::from_secs_f64(duration))
        {
            if !sample.is_finite() {
                return Err("Non-finite audio".into());
            }
            sum += (sample as f64).powi(2);
            n += 1;
        }
        if n == 0 {
            return Err("Excerpt starts after audio ends".into());
        }
        Ok((sum / n as f64).sqrt())
    }
    pub fn save_report(&self, id: &str, report: &serde_json::Value) -> Result<(), String> {
        validate_id(id)?;
        fs::write(
            self.root.join(format!("{id}.analysis.json")),
            serde_json::to_vec(report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    pub fn report(&self, id: &str) -> Result<serde_json::Value, String> {
        validate_id(id)?;
        serde_json::from_slice(
            &fs::read(self.root.join(format!("{id}.analysis.json"))).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("Invalid artifact ID".into());
    }
    Ok(())
}
/// Attenuation-only deterministic matching; neither source is amplified.
pub fn matched_gains(a: f64, b: f64) -> (f32, f32) {
    if !a.is_finite() || !b.is_finite() || a <= 1e-7 || b <= 1e-7 {
        return (1., 1.);
    }
    let target = a.min(b);
    ((target / a) as f32, (target / b) as f32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_never_boosts() {
        assert_eq!(matched_gains(0.5, 0.25), (0.5, 1.));
        assert_eq!(matched_gains(0.25, 0.5), (1., 0.5));
        assert_eq!(matched_gains(f64::NAN, 1.), (1., 1.));
    }
    #[test]
    fn paths_are_opaque() {
        assert!(validate_id("../../private").is_err());
        assert!(validate_id(&"a".repeat(64)).is_ok());
    }
}
