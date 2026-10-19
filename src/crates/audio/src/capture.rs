//! Explicit, bounded recording from a system audio input.
use cpal::{
    Device, Sample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    fmt,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

pub const MAX_CAPTURE_SECONDS: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    pub index: usize,
    pub name: String,
}

impl fmt::Display for InputDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} · {}", self.index + 1, self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Started { sample_rate: u32, channels: u16 },
    Finished(Result<PathBuf, String>),
}

enum Control {
    Stop,
    Error(String),
}

pub struct Capture {
    control: Sender<Control>,
    pub events: std::sync::Arc<std::sync::Mutex<Receiver<Event>>>,
}

impl Capture {
    pub fn start(device: InputDevice, output: PathBuf) -> Result<Self, String> {
        let (control_tx, control_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let worker_control_tx = control_tx.clone();
        thread::Builder::new()
            .name("axe-audio-capture".into())
            .spawn(move || {
                let result = capture(device, output, worker_control_tx, control_rx, &event_tx);
                let _ = event_tx.send(Event::Finished(result));
            })
            .map_err(|e| format!("Cannot start audio capture: {e}"))?;
        Ok(Self {
            control: control_tx,
            events: std::sync::Arc::new(std::sync::Mutex::new(events)),
        })
    }

    pub fn stop(&self) {
        let _ = self.control.send(Control::Stop);
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn input_devices() -> Result<Vec<InputDevice>, String> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(|e| e.to_string())?;
    Ok(devices
        .enumerate()
        .filter_map(|(index, device)| device.name().ok().map(|name| InputDevice { index, name }))
        .collect())
}

fn selected_device(selected: &InputDevice) -> Result<Device, String> {
    let host = cpal::default_host();
    let mut devices = host.input_devices().map_err(|e| e.to_string())?;
    let device = devices
        .nth(selected.index)
        .ok_or("Selected audio input is no longer available")?;
    if device.name().ok().as_deref() != Some(selected.name.as_str()) {
        return Err("Audio input devices changed; refresh the device list".into());
    }
    Ok(device)
}

fn capture(
    selected: InputDevice,
    output: PathBuf,
    control_tx: Sender<Control>,
    control_rx: Receiver<Control>,
    events: &Sender<Event>,
) -> Result<PathBuf, String> {
    let device = selected_device(&selected)?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let config: cpal::StreamConfig = supported.clone().into();
    let sample_rate = config.sample_rate.0;
    let input_channels = config.channels as usize;
    let channels = input_channels.min(2) as u16;
    if input_channels == 0 || input_channels > 32 || sample_rate == 0 || sample_rate > 192_000 {
        return Err("The selected input has an unsupported audio format".into());
    }

    let (samples_tx, samples_rx) = mpsc::channel::<Vec<f32>>();
    let error_tx = control_tx.clone();
    let error_callback = move |error| {
        let _ = error_tx.send(Control::Error(format!("Audio input stopped: {error}")));
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => {
            build_stream::<f32>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::F64 => {
            build_stream::<f64>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::I8 => {
            build_stream::<i8>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::I16 => {
            build_stream::<i16>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::I24 => {
            build_stream::<cpal::I24>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::I32 => {
            build_stream::<i32>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::I64 => {
            build_stream::<i64>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::U8 => {
            build_stream::<u8>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::U16 => {
            build_stream::<u16>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::U32 => {
            build_stream::<u32>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        SampleFormat::U64 => {
            build_stream::<u64>(&device, &config, input_channels, samples_tx, error_callback)?
        }
        _ => return Err("The selected input sample format is unsupported".into()),
    };
    stream.play().map_err(|e| e.to_string())?;
    let _ = events.send(Event::Started {
        sample_rate,
        channels,
    });

    let max_samples = sample_rate as usize * channels as usize * MAX_CAPTURE_SECONDS as usize;
    let mut samples =
        Vec::with_capacity(max_samples.min(sample_rate as usize * channels as usize * 5));
    while samples.len() < max_samples {
        match control_rx.try_recv() {
            Ok(Control::Stop) => {
                break;
            }
            Ok(Control::Error(error)) => return Err(error),
            Err(mpsc::TryRecvError::Disconnected) => {
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match samples_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(mut chunk) => {
                let remaining = max_samples.saturating_sub(samples.len());
                chunk.truncate(remaining);
                samples.extend(chunk);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Audio input disconnected during capture".into());
            }
        }
    }
    drop(stream);
    if samples.len() < sample_rate as usize * channels as usize / 4 {
        return Err("Record at least 0.25 seconds before stopping".into());
    }
    write_wav(&output, sample_rate, channels, &samples)?;
    Ok(output)
}

fn build_stream<T>(
    device: &Device,
    config: &cpal::StreamConfig,
    input_channels: usize,
    samples: Sender<Vec<f32>>,
    errors: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Sample + Copy,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let converted = data
                    .chunks_exact(input_channels)
                    .flat_map(|frame| frame.iter().take(input_channels.min(2)))
                    .map(|sample| sample.to_sample::<f32>())
                    .collect();
                let _ = samples.send(converted);
            },
            errors,
            None,
        )
        .map_err(|e| e.to_string())
}

fn write_wav(
    path: &PathBuf,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Result<(), String> {
    let data_len = samples
        .len()
        .checked_mul(2)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("Audio capture is too large")?;
    let byte_rate = sample_rate
        .checked_mul(channels as u32)
        .and_then(|n| n.checked_mul(2))
        .ok_or("Audio format is too large")?;
    let block_align = channels.checked_mul(2).ok_or("Invalid channel count")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(b"RIFF").map_err(|e| e.to_string())?;
    file.write_all(&(36_u32 + data_len).to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(b"WAVEfmt ").map_err(|e| e.to_string())?;
    file.write_all(&16_u32.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&1_u16.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&channels.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&sample_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&byte_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&block_align.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(&16_u16.to_le_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(b"data").map_err(|e| e.to_string())?;
    file.write_all(&data_len.to_le_bytes())
        .map_err(|e| e.to_string())?;
    for sample in samples {
        let sample = if sample.is_finite() {
            sample.clamp(-1., 1.)
        } else {
            0.
        };
        let pcm = if sample <= -1. {
            i16::MIN
        } else {
            (sample * i16::MAX as f32).round() as i16
        };
        file.write_all(&pcm.to_le_bytes())
            .map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
