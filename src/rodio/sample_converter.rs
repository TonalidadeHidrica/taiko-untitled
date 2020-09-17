use std::collections::VecDeque;

pub struct TrueSampleConverter<S>
where
    S: Iterator<Item = f32>,
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
    S: Iterator<Item = f32>,
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
        let remove_len =
            (sample_index - self.input_front_sample_index) as usize * self.channels as usize;
        // println!(
        //     "discard_before({}) => remove_len={}",
        //     sample_index, remove_len
        // );
        // println!(
        //     "ifsi = {}, chs = {}",
        //     self.input_front_sample_index, self.channels,
        // );
        self.input_samples_queue
            .drain(..self.input_samples_queue.len().min(remove_len));
        self.input_front_sample_index = sample_index;
        // println!("que_len = {}, ifsi = {}",
        //     self.input_samples_queue.len(),
        //     self.input_front_sample_index,
        // );
        // println!();
    }

    fn append_until(&mut self, sample_index_exclusive: u64) {
        let add_len = (sample_index_exclusive - self.input_front_sample_index) as usize
            * self.channels as usize
            - self.input_samples_queue.len();
        // println!(
        //     "append_until({}) => add_len={}",
        //     sample_index_exclusive, add_len
        // );
        // println!(
        //     "ifsi = {}, chs = {}, que_len = {}",
        //     self.input_front_sample_index,
        //     self.channels,
        //     self.input_samples_queue.len(),
        // );
        for _ in 0..add_len {
            self.input_samples_queue
                .push_back(self.source.next().unwrap_or(0.0));
        }
        // println!("que_len = {}, ifsi = {}",
        //     self.input_samples_queue.len(),
        //     self.input_front_sample_index,
        // );
        // println!();
    }

    #[inline]
    fn get(&self, sample_index: u64, channel_index: u16) -> f32 {
        let index_delta = (sample_index - self.input_front_sample_index) as usize;
        let ret =
            self.input_samples_queue[index_delta * self.channels as usize + channel_index as usize];
        // println!("{}\t{}\t{}\t=> {}\n", sample_index, channel_index, index_delta, ret);
        ret
    }
}

impl<S> Iterator for TrueSampleConverter<S>
where
    S: Iterator<Item = f32>,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.output_samples_queue.pop_front() {
            Some(next)
        } else {
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
