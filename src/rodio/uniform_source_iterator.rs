use super::channel_count_converter::ChannelCountConverter;
use super::data_converter::DataConverter;
use super::sample_converter::TrueSampleConverter;
use super::seek::Seekable;
use cpal::StreamConfig;
use rodio::{Sample, Source};

pub type TrueUniformSourceIterator<S> =
    TrueSampleConverter<ChannelCountConverter<DataConverter<S, f32>>>;

pub fn new_uniform_source_iterator<S>(
    source: S,
    stream_config: &StreamConfig,
) -> TrueUniformSourceIterator<S>
where
    S: Source + Seekable,
    S::Item: Sample + cpal::Sample,
{
    let input_sample_rate = source.sample_rate();
    let input_channels = source.channels();

    let source = DataConverter::<_, f32>::new(source);
    let source = ChannelCountConverter::new(source, input_channels, stream_config.channels);
    TrueSampleConverter::new(
        source,
        input_sample_rate,
        stream_config.channels,
        stream_config.sample_rate.0,
    )
}
