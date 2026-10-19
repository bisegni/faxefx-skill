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
    pub fn list(&self) -> Result<Vec<Artifact>, String> {
        let mut artifacts = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if validate_id(id).is_ok()
                && let Ok(artifact) = self.get(id)
            {
                artifacts.push(artifact);
            }
        }
        artifacts.sort_by_key(|a| a.name.to_lowercase());
        Ok(artifacts)
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
    /// Produce a bounded mono WAV preview suitable for explicit sharing with an agent.
    /// Source audio is decoded locally and never persisted in this normalized form.
    pub fn agent_audio(&self, id: &str, start: f64, duration: f64) -> Result<String, String> {
        let artifact = self.get(id)?;
        if !start.is_finite()
            || start < 0.0
            || !duration.is_finite()
            || !(0.25..=30.0).contains(&duration)
            || start + duration > artifact.duration + 0.001
        {
            return Err("Agent audio excerpt must be 0.25–30 seconds within the sample".into());
        }
        let decoder =
            rodio::Decoder::try_from(fs::File::open(self.path(id)?).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let rate = decoder.sample_rate();
        let channels = decoder.channels() as usize;
        if !(8_000..=192_000).contains(&rate) || !(1..=8).contains(&channels) {
            return Err("Audio format is outside supported sharing limits".into());
        }
        let samples: Vec<f32> = decoder
            .skip_duration(std::time::Duration::from_secs_f64(start))
            .take_duration(std::time::Duration::from_secs_f64(duration))
            .collect();
        let frames: Vec<f32> = samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
            .collect();
        if frames.len() < (rate as f64 * 0.25) as usize {
            return Err("Audio excerpt is too short to share".into());
        }
        let output_rate = 24_000_u32;
        let output_count = (frames.len() as u64 * output_rate as u64 / rate as u64) as usize;
        let mut pcm = Vec::with_capacity(output_count * 2);
        for index in 0..output_count {
            let position = index as f64 * rate as f64 / output_rate as f64;
            let left = position.floor() as usize;
            let right = (left + 1).min(frames.len() - 1);
            let frac = (position - left as f64) as f32;
            let value = frames[left] * (1.0 - frac) + frames[right] * frac;
            let value = if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            let value = if value <= -1.0 {
                i16::MIN
            } else {
                (value * i16::MAX as f32).round() as i16
            };
            pcm.extend_from_slice(&value.to_le_bytes());
        }
        let data_len = u32::try_from(pcm.len()).map_err(|_| "Audio excerpt is too large")?;
        let mut wav = Vec::with_capacity(44 + pcm.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&output_rate.to_le_bytes());
        wav.extend_from_slice(&(output_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(&pcm);
        Ok(base64_encode(&wav))
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
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
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
