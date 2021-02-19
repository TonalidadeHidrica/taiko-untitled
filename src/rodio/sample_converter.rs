use super::seek::Seekable;
use std::collections::VecDeque;

pub struct TrueSampleConverter<S>
where
    S: Iterator<Item = f32> + Seekable,
{
    source: S,
    channels: u16,
    input_sample_rate: f64,
    output_sample_rate: f64,

    input_samples_queue: VecDeque<S::Item>,
    input_front_sample_index: u64,
    output_samples_queue: VecDeque<S::Item>,
    output_next_sample_index: u64,
}

impl<S> TrueSampleConverter<S>
where
    S: Iterator<Item = f32> + Seekable,
{
    pub fn new(
        source: S,
        input_sample_rate: u32,
        input_channels: u16,
        output_sample_rate: u32,
    ) -> Self {
        assert!(input_channels > 0);
        Self {
            channels: input_channels,
            input_sample_rate: input_sample_rate as f64,
            source,
            output_sample_rate: output_sample_rate as f64,

            input_samples_queue: Default::default(),
            input_front_sample_index: 0,
            output_samples_queue: Default::default(),
            output_next_sample_index: 0,
        }
    }

    #[inline]
    fn discard_before(&mut self, sample_index: u64) {
        // TODO is saturating_sub correct?
        let remove_len = sample_index.saturating_sub(self.input_front_sample_index) as usize
            * self.channels as usize;
        self.input_samples_queue
            .drain(..self.input_samples_queue.len().min(remove_len));
        self.input_front_sample_index = sample_index;
    }

    fn append_until(&mut self, sample_index_exclusive: u64) {
        let add_len = (sample_index_exclusive - self.input_front_sample_index) as usize
            * self.channels as usize
            - self.input_samples_queue.len();
        for _ in 0..add_len {
            self.input_samples_queue
                .push_back(self.source.next().unwrap_or(0.0));
        }
    }

    #[inline]
    fn get(&self, sample_index: u64, channel_index: u16) -> f32 {
        let index_delta = (sample_index - self.input_front_sample_index) as usize;
        self.input_samples_queue[index_delta * self.channels as usize + channel_index as usize]
    }

    /// if time < 0, then seek to 0
    pub fn seek(&mut self, time: f64) -> Result<u64, String> {
        let time = time.max(0.0);
        self.input_front_sample_index = (time * self.input_sample_rate) as u64;
        self.output_next_sample_index = (time * self.output_sample_rate) as u64 + 1;
        self.input_samples_queue.clear();
        self.output_samples_queue.clear();
        self.source.seek(self.input_front_sample_index)?;
        Ok(self.output_next_sample_index)
    }

    pub fn output_sample_rate(&self) -> f64 {
        self.output_sample_rate
    }

    pub fn set_output_sample_rate(&mut self, rate: f64) {
        self.output_sample_rate = rate;
    }
}

impl<S> Iterator for TrueSampleConverter<S>
where
    S: Iterator<Item = f32> + Seekable,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.output_samples_queue.pop_front() {
            Some(next)
        } else {
            // TODO there may be a precision issue here (although after very long).
            let next_index = self.output_next_sample_index as f64 / self.output_sample_rate
                * self.input_sample_rate;
            let int = next_index.trunc() as u64;
            let fract = next_index.fract() as f32;
            self.discard_before(int);
            self.append_until(int + 2);
            for i in 0..self.channels {
                let next = self.get(int, i) * (1.0 - fract) + self.get(int + 1, i) * fract;
                self.output_samples_queue.push_back(next);
            }
            self.output_next_sample_index += 1;
            self.next()
        }
    }
}
