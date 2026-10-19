use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, FromSample, OutputCallbackInfo, Sample, SampleFormat, SizedSample, Stream,
    StreamConfig,
};

use crate::error::AudioError;
use crate::mixer::{AudioHandle, Mixer};

const PREFERRED_BUFFER_FRAMES: u32 = 256;
const SCRATCH_FRAMES: usize = 8192;

pub struct AudioEngine {
    _stream: Stream,
    sample_rate: u32,
    buffer_frames: Option<u32>,
}

impl std::fmt::Debug for AudioEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioEngine")
            .field("sample_rate", &self.sample_rate)
            .field("buffer_frames", &self.buffer_frames)
            .finish()
    }
}

fn device_err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Device(e.to_string())
}

/// Output device names, as `soundDevice` in `options.txt` stores them.
pub fn output_devices() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|devices| devices.map(|d| d.to_string()).take(64).collect())
        .unwrap_or_default()
}

fn pick_device(name: Option<&str>) -> Option<Device> {
    let host = cpal::default_host();
    if let Some(name) = name.filter(|n| !n.is_empty())
        && let Ok(mut devices) = host.output_devices()
        && let Some(device) = devices.find(|d| d.to_string() == name)
    {
        return Some(device);
    }
    host.default_output_device()
}

impl AudioEngine {
    /// Opens `device` by name, or the system default when it is unset or gone.
    pub fn start(device: Option<&str>) -> Result<(Self, AudioHandle), AudioError> {
        let device =
            pick_device(device).ok_or_else(|| AudioError::Device("no output device".into()))?;
        let supported = device.default_output_config().map_err(device_err)?;
        let format = supported.sample_format();
        let base: StreamConfig = supported.into();

        let mut last_err = None;
        for buffer in [
            BufferSize::Fixed(PREFERRED_BUFFER_FRAMES),
            BufferSize::Default,
        ] {
            let config = StreamConfig {
                buffer_size: buffer,
                ..base
            };
            let (mixer, handle) = Mixer::with_queue(config.sample_rate);
            match build(&device, &config, format, mixer) {
                Ok(stream) => {
                    stream.play().map_err(device_err)?;
                    let buffer_frames = match buffer {
                        BufferSize::Fixed(n) => Some(n),
                        BufferSize::Default => None,
                    };
                    let engine = Self {
                        _stream: stream,
                        sample_rate: config.sample_rate,
                        buffer_frames,
                    };
                    return Ok((engine, handle));
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| AudioError::Device("no usable stream config".into())))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Output buffer latency in milliseconds when the device honoured the fixed buffer size.
    pub fn buffer_latency_ms(&self) -> Option<f32> {
        self.buffer_frames
            .map(|n| n as f32 * 1000.0 / self.sample_rate as f32)
    }
}

fn build(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    mixer: Mixer,
) -> Result<Stream, AudioError> {
    match format {
        SampleFormat::F32 => build_typed::<f32>(device, config, mixer),
        SampleFormat::I16 => build_typed::<i16>(device, config, mixer),
        SampleFormat::U16 => build_typed::<u16>(device, config, mixer),
        SampleFormat::I32 => build_typed::<i32>(device, config, mixer),
        other => Err(AudioError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }
}

fn build_typed<T>(
    device: &Device,
    config: &StreamConfig,
    mut mixer: Mixer,
) -> Result<Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * 2];
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &OutputCallbackInfo| {
                for block in data.chunks_mut(SCRATCH_FRAMES * channels) {
                    let frames = block.len() / channels;
                    let stereo = &mut scratch[..frames * 2];
                    mixer.render(stereo);
                    for (out, s) in block
                        .chunks_exact_mut(channels)
                        .zip(stereo.as_chunks::<2>().0)
                    {
                        write_frame(out, s[0], s[1]);
                    }
                }
            },
            |e| tracing::warn!("audio stream error: {e}"),
            None,
        )
        .map_err(device_err)
}

fn write_frame<T: Sample + FromSample<f32>>(out: &mut [T], l: f32, r: f32) {
    let (l, r) = (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0));
    match out {
        [mono] => *mono = T::from_sample((l + r) * 0.5),
        [left, right, rest @ ..] => {
            *left = T::from_sample(l);
            *right = T::from_sample(r);
            rest.iter_mut().for_each(|s| *s = T::from_sample(0.0));
        }
        [] => {}
    }
}
