use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SampleFormat, SizedSample, Stream,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin";
const MODEL_MIN_BYTES: u64 = 70 * 1024 * 1024;
const WHISPER_SAMPLE_RATE: u32 = 16_000;

pub struct Recording {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    stream_error: Arc<Mutex<Option<String>>>,
}

pub async fn prepare_model(path: PathBuf) -> Result<(), String> {
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() >= MODEL_MIN_BYTES => return Ok(()),
        Ok(_) => fs::remove_file(&path)
            .map_err(|error| format!("Could not remove the incomplete speech model: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not check the speech model file: {error}")),
    }

    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("Could not create the speech model folder: {error}"))?;
    }

    let response = reqwest::get(MODEL_URL)
        .await
        .map_err(|error| format!("Could not download the offline speech model: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Speech model download failed: {error}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read the downloaded speech model: {error}"))?;
    if (bytes.len() as u64) < MODEL_MIN_BYTES {
        return Err(format!(
            "Downloaded speech model is incomplete ({} bytes). Please try again.",
            bytes.len()
        ));
    }

    let temporary_path = path.with_extension("download");
    fs::write(&temporary_path, &bytes)
        .map_err(|error| format!("Could not save the speech model: {error}"))?;
    fs::rename(&temporary_path, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary_path);
        format!("Could not install the speech model: {error}")
    })?;
    Ok(())
}

pub fn start_recording() -> Result<Recording, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone is available.".to_string())?;
    let input_config = device
        .default_input_config()
        .map_err(|error| format!("Could not configure the microphone: {error}"))?;
    let sample_rate = input_config.sample_rate();
    let channels = input_config.channels();
    let sample_format = input_config.sample_format();
    let stream_config: cpal::StreamConfig = input_config.into();
    let samples = Arc::new(Mutex::new(Vec::new()));
    let stream_error = Arc::new(Mutex::new(None));

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::F64 => build_stream::<f64>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::I8 => build_stream::<i8>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::I24 => build_stream::<cpal::I24>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::I32 => build_stream::<i32>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::I64 => build_stream::<i64>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::U8 => build_stream::<u8>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::U24 => build_stream::<cpal::U24>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::U32 => build_stream::<u32>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        SampleFormat::U64 => build_stream::<u64>(
            &device,
            stream_config,
            samples.clone(),
            stream_error.clone(),
        ),
        format => {
            return Err(format!(
                "The microphone's audio format ({format}) is not supported."
            ))
        }
    }?;

    stream
        .play()
        .map_err(|error| format!("Could not start microphone recording: {error}"))?;

    Ok(Recording {
        stream,
        samples,
        sample_rate,
        channels,
        stream_error,
    })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<Stream, String>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    let error_state = stream_error.clone();
    device
        .build_input_stream(
            config,
            move |input: &[T], _| {
                let mut output = match samples.lock() {
                    Ok(output) => output,
                    Err(_) => {
                        if let Ok(mut error) = stream_error.lock() {
                            *error =
                                Some("Microphone audio buffer became unavailable.".to_string());
                        }
                        return;
                    }
                };
                output.extend(input.iter().copied().map(f32::from_sample));
            },
            move |error| {
                if let Ok(mut state) = error_state.lock() {
                    *state = Some(error.to_string());
                }
            },
            None::<Duration>,
        )
        .map_err(|error| format!("Could not open the microphone: {error}"))
}

pub fn transcribe(recording: Recording, model_path: PathBuf) -> Result<String, String> {
    let Recording {
        stream,
        samples,
        sample_rate,
        channels,
        stream_error,
    } = recording;
    drop(stream);

    if let Some(error) = stream_error
        .lock()
        .map_err(|_| "Microphone error state became unavailable.".to_string())?
        .take()
    {
        return Err(format!("Microphone recording failed: {error}"));
    }

    let samples = samples
        .lock()
        .map_err(|_| "Recorded microphone audio became unavailable.".to_string())?
        .clone();
    let audio = convert_to_whisper_audio(&samples, sample_rate, channels)?;
    if audio.len() < WHISPER_SAMPLE_RATE as usize / 5 {
        return Err("No speech was captured. Please speak and try again.".to_string());
    }

    let context = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .map_err(|error| format!("Could not load the local speech model: {error}"))?;
    let mut state = context
        .create_state()
        .map_err(|error| format!("Could not initialize speech transcription: {error}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_n_threads(
        std::thread::available_parallelism()
            .map(|count| count.get().min(4) as i32)
            .unwrap_or(2),
    );
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state
        .full(params, &audio)
        .map_err(|error| format!("Local speech transcription failed: {error}"))?;

    let transcript = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<String>()
        .trim()
        .to_string();
    if transcript.is_empty() {
        return Err("No speech was recognized. Please speak and try again.".to_string());
    }
    Ok(transcript)
}

fn convert_to_whisper_audio(
    interleaved: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, String> {
    if sample_rate == 0 || channels == 0 {
        return Err("The microphone returned an invalid audio format.".to_string());
    }

    let channel_count = channels as usize;
    let mono = interleaved
        .chunks_exact(channel_count)
        .map(|frame| frame.iter().sum::<f32>() / channel_count as f32)
        .collect::<Vec<_>>();
    if sample_rate == WHISPER_SAMPLE_RATE {
        return Ok(mono);
    }

    let output_len = (mono.len() as u64 * WHISPER_SAMPLE_RATE as u64 / sample_rate as u64) as usize;
    let scale = sample_rate as f64 / WHISPER_SAMPLE_RATE as f64;
    Ok((0..output_len)
        .map(|index| {
            let source = index as f64 * scale;
            let left = source.floor() as usize;
            let right = (left + 1).min(mono.len().saturating_sub(1));
            let fraction = (source - left as f64) as f32;
            mono[left] * (1.0 - fraction) + mono[right] * fraction
        })
        .collect())
}

pub async fn speak(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut speech =
            tts::Tts::default().map_err(|error| format!("Speech output failed: {error}"))?;
        speech
            .speak(&text, true)
            .map(|_| ())
            .map_err(|error| format!("Speech output failed: {error}"))
    })
    .await
    .map_err(|error| format!("Speech output task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{convert_to_whisper_audio, WHISPER_SAMPLE_RATE};

    #[test]
    fn converts_stereo_audio_to_mono_at_whisper_rate() {
        let input = vec![0.5, -0.5, 1.0, 0.0, -1.0, 0.5, 0.0, 0.0];
        let converted = convert_to_whisper_audio(&input, WHISPER_SAMPLE_RATE, 2).unwrap();
        assert_eq!(converted.len(), 4);
        assert_eq!(converted, vec![0.0, 0.5, -0.25, 0.0]);
    }

    #[test]
    fn resamples_audio_to_whisper_rate() {
        let input = (0..48_000).map(|sample| sample as f32).collect::<Vec<_>>();
        let converted = convert_to_whisper_audio(&input, 48_000, 1).unwrap();
        assert_eq!(converted.len(), 16_000);
        assert_eq!(converted[1], 3.0);
    }

    #[test]
    fn rejects_invalid_audio_format() {
        assert!(convert_to_whisper_audio(&[], 0, 1).is_err());
        assert!(convert_to_whisper_audio(&[], 16_000, 0).is_err());
    }
}
