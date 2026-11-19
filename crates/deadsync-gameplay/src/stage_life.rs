/// PlayerStageStats history uses accumulated steps time, independently of the
/// music-time history used by the gameplay graph.
#[derive(Clone, Debug, Default)]
pub struct StageLifeRecord {
    pub steps_seconds: f32,
    pub samples: Vec<(f32, f32)>,
}

impl StageLifeRecord {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            steps_seconds: 0.0,
            samples: Vec::with_capacity(capacity),
        }
    }

    pub fn copy_from(&mut self, source: &Self) {
        self.steps_seconds = source.steps_seconds;
        self.samples.clone_from(&source.samples);
    }

    pub fn clear(&mut self) {
        self.steps_seconds = 0.0;
        self.samples.clear();
    }

    /// Player::ChangeLife(TapNoteScore/HoldNoteScore) records the resulting life
    /// once. Its float ChangeLife/SetLife overloads record before and after.
    pub fn record(&mut self, life: f32) {
        self.record_at(self.steps_seconds, life);
    }

    /// Explicit ScreenGameplay samples use their own timestamp without
    /// changing the clock used by subsequent Player life changes.
    pub fn record_at(&mut self, seconds: f32, life: f32) {
        if seconds < 0.0 {
            return;
        }
        let index = self.samples.partition_point(|&(time, _)| time < seconds);
        if let Some(&(time, previous)) = self.samples.get(index)
            && time == seconds
            && previous != life
        {
            self.set_sample(seconds - 0.00390625, previous);
        }
        self.set_sample(seconds, life);
        let len = self.samples.len();
        if len >= 3
            && self.samples[len - 3].1 == self.samples[len - 2].1
            && self.samples[len - 2].1 == self.samples[len - 1].1
        {
            self.samples.remove(len - 2);
        }
    }

    fn set_sample(&mut self, seconds: f32, life: f32) {
        let index = self.samples.partition_point(|&(time, _)| time < seconds);
        if self
            .samples
            .get(index)
            .is_some_and(|&(time, _)| time == seconds)
        {
            self.samples[index].1 = life;
        } else {
            self.samples.insert(index, (seconds, life));
        }
    }

    /// PlayerStageStats::GetLifeRecordLerpAt and RageUtil's float SCALE order.
    pub fn sample(&self, seconds: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let later = self.samples.partition_point(|&(time, _)| !(seconds < time));
        let earlier = later.saturating_sub(1);
        let (from, before) = self.samples[earlier];
        let Some(&(to, after)) = self.samples.get(later) else {
            return before;
        };
        if from == to {
            return before;
        }
        (seconds - from) * (after - before) / (to - from) + before
    }
}
