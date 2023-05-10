use crate::ImageHandle;
use cgmath::{Array, Vector3, Zero};
use log::info;

pub struct ParticleSystem {
    particles: Vec<Particle>,
    state: ParticleSystemState,
    time_since_last_spawn: f32,
    pub spawn_rate: f32,
    pub spawn_position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub initial_colour: Vector3<f32>,
}

impl ParticleSystem {
    pub fn new(particle_limit: usize) -> Self {
        let mut particles = Vec::default();
        particles.resize(particle_limit, Particle::default());
        Self {
            particles,
            ..Default::default()
        }
    }

    pub fn set_state(&mut self, state: ParticleSystemState) {
        self.state = state
    }

    pub fn tick(&mut self, delta_time: f32) {
        self.time_since_last_spawn += delta_time;
        // Loop incase fallen behind
        for x in 0..10 {
            if self.time_since_last_spawn >= self.spawn_rate {
                let unused_particle_index = self.first_unused_particle();
                self.spawn_particle(unused_particle_index);
                self.time_since_last_spawn -= self.spawn_rate;
            } else {
                break;
            }
        }

        for particle in self.particles.iter_mut() {
            particle.life -= delta_time;
            if particle.life >= 0.0 {
                particle.position += particle.velocity * delta_time;
            }
        }
    }

    pub fn particles(&self) -> Vec<&Particle> {
        self.particles
            .iter()
            .filter(|particle| particle.life >= 0.0)
            .collect()
    }

    fn first_unused_particle(&self) -> usize {
        for (i, particle) in self.particles.iter().enumerate() {
            if particle.life <= 0.0 {
                return i;
            }
        }
        0
    }

    fn spawn_particle(&mut self, particle_index: usize) {
        let mut particle = &mut self.particles[particle_index];
        particle.position = self.spawn_position;
        particle.velocity = self.velocity;
        particle.life = 5.0;
        particle.colour = self.initial_colour;
    }
}

impl Default for ParticleSystem {
    fn default() -> Self {
        let particle_limit = 64;
        let mut particles = Vec::default();
        particles.resize(particle_limit, Particle::default());
        Self {
            particles,
            time_since_last_spawn: 0.0,
            spawn_rate: 1.0,
            spawn_position: Vector3::zero(),
            velocity: Vector3::new(0.0, 1.0, 0.0),
            state: ParticleSystemState::Stopped,
            initial_colour: Vector3::from_value(1.0),
        }
    }
}

pub enum ParticleSystemState {
    Stopped,
    Running,
}

#[derive(Copy, Clone)]
pub struct Particle {
    pub life: f32,
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub texture_index: Option<ImageHandle>,
    pub colour: Vector3<f32>,
    pub size: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            life: 0.0f32,
            position: Vector3::zero(),
            velocity: Vector3::zero(),
            texture_index: None,
            colour: Vector3::from_value(1f32),
            size: 0.25,
        }
    }
}
