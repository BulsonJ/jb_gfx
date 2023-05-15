use std::time::Instant;

pub struct FrameTimer {
    frame_start_time: Instant,
    frame_time: f32,
    delta_time: f32,
    target_frame_time: f32,
    total_time_elapsed: f32,
}

impl FrameTimer {
    pub fn new() -> Self {
        FrameTimer::default()
    }

    pub fn update(&mut self) {
        self.frame_time = self.frame_start_time.elapsed().as_secs_f32();
        self.frame_start_time = Instant::now();
    }

    pub fn sub_frame_update(&mut self) -> bool {
        if self.frame_time > 0.0f32 {
            let delta_time = self.frame_time.min(self.target_frame_time);

            self.delta_time = delta_time;
            self.frame_time -= delta_time;
            self.total_time_elapsed += delta_time;

            true
        } else {
            false
        }
    }

    pub fn total_time_elapsed(&self) -> f32 {
        self.total_time_elapsed
    }

    pub fn delta_time(&self) -> f32 {
        self.delta_time
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self {
            frame_start_time: Instant::now(),
            frame_time: 0.0,
            delta_time: 0.0,
            target_frame_time: 1.0 / 120.0,
            total_time_elapsed: 0.0,
        }
    }
}
