use std::time::Duration;
use std::time::Instant;

pub struct ValueWithUpdateTime<T> {
    value: T,
    time: Instant,
}

impl<T> ValueWithUpdateTime<T> {
    pub fn update<'a, F>(&'a mut self, f: F)
    where
        F: FnOnce(&'a mut T),
    {
        f(&mut self.value);
        self.time = Instant::now();
    }

    pub fn last_update(&self) -> Instant {
        self.time
    }

    pub fn duration_since_update(&self) -> Duration {
        Instant::now() - self.time
    }
}

impl<T: Copy> ValueWithUpdateTime<T> {
    pub fn new(value: T) -> Self {
        Self::new_with_time(value, Instant::now())
    }

    pub fn new_with_time(value: T, time: Instant) -> Self {
        Self { value, time }
    }

    pub fn get(&self) -> T {
        self.value
    }

    pub fn set(&mut self, value: T) {
        *self = Self::new(value);
    }

    pub fn set_with_time(&mut self, value: T, time: Instant) {
        *self = Self::new_with_time(value, time);
    }

    pub fn set_with<F>(&mut self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        self.set(f(self.value))
    }
}

// impl <T> Deref for ValueWithUpdateTime<T> {
//     type Target = T;
//
//     fn deref(&self) -> &Self::Target {
//         &self.value
//     }
// }
//
// impl <T> DerefMut for ValueWithUpdateTime<T> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         self.time = Instant::now();
//         &mut self.value
//     }
// }

pub struct EasingF64Impl<F> {
    old_value: f64,
    value: ValueWithUpdateTime<f64>,
    easing_time: Duration,
    easing_function: F,
}

impl<F> EasingF64Impl<F>
where
    F: Fn(f64) -> f64,
{
    pub fn new(value: f64, easing_time: Duration, easing_function: F) -> Self {
        Self {
            old_value: value,
            value: ValueWithUpdateTime::new(value),
            easing_time,
            easing_function,
        }
    }
}

pub trait EasingF64 {
    fn get(&self) -> f64;
    fn get_eased(&self) -> f64;
    fn set(&mut self, value: f64);
    fn set_with<F: FnOnce(f64) -> f64>(&mut self, f: F) {
        self.set(f(self.get()));
    }
}

impl<F> EasingF64 for EasingF64Impl<F>
where
    F: Fn(f64) -> f64,
{
    fn get(&self) -> f64 {
        self.value.get()
    }

    fn get_eased(&self) -> f64 {
        let t = self.value.duration_since_update().as_secs_f64() / self.easing_time.as_secs_f64();
        let t = (self.easing_function)(t.clamp(0.0, 1.0));
        self.value.get() * t + self.old_value * (1.0 - t)
    }

    fn set(&mut self, value: f64) {
        self.old_value = self.get_eased();
        self.value.set(value);
    }
}
