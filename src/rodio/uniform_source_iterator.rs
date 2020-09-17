use super::channel_count_converter::ChannelCountConverter;
use super::data_converter::DataConverter;
use super::sample_converter::TrueSampleConverter;
use cpal::StreamConfig;
use rodio::{Sample, Source};

// pub struct TrueUniformSourceIterator<S>
// where
//     S: Source,
//     S::Item: Sample + cpal::Sample,
// {
//     source: S,
// }
//
// impl<S> TrueUniformSourceIterator<S>
// where
//     S: Source,
//     S::Item: Sample + cpal::Sample,
// {

pub type TrueUniformSourceIterator<S> =
    TrueSampleConverter<ChannelCountConverter<DataConverter<S, f32>>>;

pub fn new_uniform_source_iterator<S>(
    source: S,
    stream_config: &StreamConfig,
) -> TrueUniformSourceIterator<S>
where
    S: Source,
    S::Item: Sample + cpal::Sample,
{
    let input_sample_rate = source.sample_rate();
    let input_channels = source.channels();

    let source = DataConverter::<_, f32>::new(source);
    let source = ChannelCountConverter::new(source, input_channels, stream_config.channels);
    let source = TrueSampleConverter::new(
        source,
        input_sample_rate,
        stream_config.channels,
        stream_config.sample_rate.0,
    );
    // use itertools::Itertools;
    // println!("{}", source.take(96000).enumerate().map(|(i, v)| format!("{}\t{}", i, v)).join("\n"));
    source
    // unimplemented!()
}

// }
