//! v0.16 Track B: Java 26.3 `FireworkParticles` (client): the `Starter`
//! that bursts a rocket's stars over two ticks each in their shapes
//! (small and large balls, star, creeper face, burst), the `SparkParticle`s
//! with their fade colour, trail and twinkle, and the `OverlayParticle`
//! flash. Sparks are drawn with an existing sprite in their colours.

use glam::DVec3;

use super::{Kind, Particle, System, Texture};
use crate::content::fireworks::{Explosion, Shape};
use crate::entity::Rng;

/// `SparkParticle`'s state beyond an ordinary particle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    /// `setFadeColor`: the colour it eases toward in its second half.
    pub fade: Option<[f32; 3]>,
    /// `setTrail`: leaves a still copy every other tick in its first half.
    pub trail: bool,
    /// `setTwinkle`: blinks in its last two thirds.
    pub twinkle: bool,
}

/// `Starter.CREEPER_PARTICLE_COORDS` and `STAR_PARTICLE_COORDS`.
const CREEPER: [[f64; 2]; 12] = [
    [0.0, 0.2],
    [0.2, 0.2],
    [0.2, 0.6],
    [0.6, 0.6],
    [0.6, 0.2],
    [0.2, 0.2],
    [0.2, 0.0],
    [0.4, 0.0],
    [0.4, -0.6],
    [0.2, -0.6],
    [0.2, -0.4],
    [0.0, -0.4],
];
const STAR: [[f64; 2]; 6] = [
    [0.0, 1.0],
    [0.3455, 0.309],
    [0.9511, 0.309],
    [0.379_591_836_734_693_9, -0.126_530_612_244_897_95],
    [0.612_244_897_959_183_7, -0.804_081_632_653_061_2],
    [0.0, -0.359_183_673_469_387_73],
];

/// A burst in progress: Java's `Starter` particle.
#[derive(Clone, Copy, Debug)]
pub(super) struct Starter {
    pos: DVec3,
    vel: DVec3,
    explosions: &'static [Explosion],
    life: u32,
    lifetime: u32,
}

impl Starter {
    pub(super) fn new(pos: DVec3, vel: DVec3, explosions: &'static [Explosion]) -> Self {
        let lifetime = crate::entity::firework::starter_lifetime(explosions);
        Self { pos, vel, explosions, life: 0, lifetime }
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [(c >> 16 & 255) as f32 / 255.0, (c >> 8 & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

/// Java's `Random.nextDouble`-style draws from the particle RNG.
fn unit(rng: &mut Rng) -> f64 {
    rng.next_f32() as f64
}

/// `Starter.createParticle`: a `FIREWORK` spark with Java's own velocity
/// (`SparkParticle` sets it directly), a random colour of the star's and a
/// random fade colour.
fn spark(
    rng: &mut Rng,
    pos: DVec3,
    vel: DVec3,
    colours: &[u32],
    fades: &[u32],
    trail: bool,
    twinkle: bool,
) -> Particle {
    let mut p = Particle::new(pos, Texture::Sprite(Kind::FireworkSpark.sprite()));
    p.style = Kind::FireworkSpark;
    p.velocity = vel;
    p.friction = 0.91;
    p.gravity = 0.1;
    p.emissive = true;
    p.size = 0.1 * (rng.next_f32() * 0.5 + 0.5) * 2.0 * 0.75;
    p.lifetime = 48 + rng.next_int(12) as u16;
    let c = colours[rng.next_int(colours.len() as u32) as usize];
    let [r, g, b] = rgb(c);
    p.color = [r, g, b, 0.99];
    let fade = (!fades.is_empty()).then(|| rgb(fades[rng.next_int(fades.len() as u32) as usize]));
    p.spark = Some(Spark { fade, trail, twinkle });
    p
}

/// The default spark of a rocket's trail (`ParticleTypes.FIREWORK`): white.
pub(super) fn trail_spark(rng: &mut Rng, pos: DVec3, vel: DVec3) -> Particle {
    spark(rng, pos, vel, &[0xFF_FFFF], &[], false, false)
}

impl System {
    /// Bursts in progress advance; each spawns its stars two ticks apart.
    pub(super) fn tick_fireworks(&mut self) {
        let n = self.fireworks.len();
        for _ in 0..n {
            let Some(mut s) = self.fireworks.pop_front() else { break };
            if s.life.is_multiple_of(2)
                && let Some(e) = s.explosions.get((s.life / 2) as usize)
            {
                self.burst_star(&s, e);
            }
            s.life += 1;
            if s.life <= s.lifetime {
                self.fireworks.push_back(s);
            }
        }
    }

    fn burst_star(&mut self, s: &Starter, e: &Explosion) {
        // A star without colours bursts black.
        let black = [crate::content::fireworks::firework_color(crate::color::DyeColor::Black)];
        let colours: &[u32] = if e.colors.is_empty() { &black } else { &e.colors };
        let fades = e.fade_colors.as_slice();
        let (trail, twinkle) = (e.trail, e.twinkle);
        let emit = |sys: &mut System, vel: DVec3| {
            let p = spark(&mut sys.rng, s.pos, vel, colours, fades, trail, twinkle);
            sys.pool.spawn(p);
        };
        match e.shape {
            Shape::SmallBall | Shape::LargeBall => {
                // `createParticleBall`: the surface of a cube of steps.
                let (speed, steps) = if e.shape == Shape::SmallBall { (0.25, 2) } else { (0.5, 4) };
                for y in -steps..=steps {
                    for x in -steps..=steps {
                        let mut z = -steps;
                        while z <= steps {
                            let rng = &mut self.rng;
                            let xa = f64::from(x) + (unit(rng) - unit(rng)) * 0.5;
                            let ya = f64::from(y) + (unit(rng) - unit(rng)) * 0.5;
                            let za = f64::from(z) + (unit(rng) - unit(rng)) * 0.5;
                            let len = (xa * xa + ya * ya + za * za).sqrt() / speed + super::types::gaussian(rng) * 0.05;
                            emit(self, DVec3::new(xa / len, ya / len, za / len));
                            if y != -steps && y != steps && x != -steps && x != steps {
                                z += steps * 2 - 1;
                            }
                            z += 1;
                        }
                    }
                }
            }
            Shape::Star | Shape::Creeper => {
                // `createParticleShape`: the outline turned three times.
                let (coords, flat): (&[[f64; 2]], bool) =
                    if e.shape == Shape::Star { (&STAR, false) } else { (&CREEPER, true) };
                let speed = 0.5;
                let [sx, sy] = coords[0];
                emit(self, DVec3::new(sx * speed, sy * speed, 0.0));
                let base = f64::from(self.rng.next_f32() * std::f32::consts::PI);
                let modifier = if flat { 0.034 } else { 0.34 };
                for step in 0..3 {
                    let angle = base + f64::from(step as f32 * std::f32::consts::PI) * modifier;
                    let (mut ox, mut oy) = (sx, sy);
                    for &[tx, ty] in &coords[1..] {
                        let mut sub = 0.25;
                        while sub <= 1.0 {
                            let mut xa = (ox + (tx - ox) * sub) * speed;
                            let ya = (oy + (ty - oy) * sub) * speed;
                            let za = xa * angle.sin();
                            xa *= angle.cos();
                            for flip in [-1.0, 1.0] {
                                emit(self, DVec3::new(xa * flip, ya, za * flip));
                            }
                            sub += 0.25;
                        }
                        (ox, oy) = (tx, ty);
                    }
                }
            }
            Shape::Burst => {
                // `createParticleBurst`: seventy sparks along the rocket's flight.
                let off_x = super::types::gaussian(&mut self.rng) * 0.05;
                let off_z = super::types::gaussian(&mut self.rng) * 0.05;
                for _ in 0..70 {
                    let rng = &mut self.rng;
                    let xa = s.vel.x * 0.5 + super::types::gaussian(rng) * 0.15 + off_x;
                    let za = s.vel.z * 0.5 + super::types::gaussian(rng) * 0.15 + off_z;
                    let ya = s.vel.y * 0.5 + unit(rng) * 0.5;
                    emit(self, DVec3::new(xa, ya, za));
                }
            }
        }
        // The `FLASH` overlay in the first colour.
        let mut flash = Particle::new(s.pos, Texture::Sprite(Kind::FireworkFlash.sprite()));
        flash.style = Kind::FireworkFlash;
        flash.physics = false;
        flash.emissive = true;
        flash.lifetime = 4;
        let [r, g, b] = rgb(colours[0]);
        flash.color = [r, g, b, 0.6];
        flash.size = 0.0;
        self.pool.spawn(flash);
    }
}

/// `OverlayParticle`: four ticks, 7.1 times `sin(age / 4 * pi)` across,
/// fading from 0.6 (sampled mid-tick).
pub(super) fn tick_flash(p: &mut Particle) {
    let t = p.age as f32 - 0.5;
    p.size = 7.1 * (t * 0.25 * std::f32::consts::PI).sin().max(0.0);
    p.color[3] = (0.6 - t * 0.25 * 0.5).max(0.0);
}

/// `SimpleAnimatedParticle.tick` and `SparkParticle.tick` after the move:
/// the second half fades out toward the fade colour, a twinkling spark
/// blinks, and a trailing spark leaves a still copy every other tick.
pub(super) fn after_spark_tick(p: &mut Particle) -> Option<Particle> {
    let spark = p.spark?;
    let (age, life) = (u32::from(p.age), u32::from(p.lifetime));
    let mut alpha = 0.99;
    if age > life / 2 {
        alpha = 1.0 - (age as f32 - (life / 2) as f32) / life as f32;
        if let Some(fade) = spark.fade {
            for (c, f) in p.color.iter_mut().zip(fade) {
                *c += (f - *c) * 0.2;
            }
        }
    }
    // `SparkParticle.extract`: hidden on alternate thirds-of-ticks late on.
    let shown = !spark.twinkle || age < life / 3 || ((age + life) / 3).is_multiple_of(2);
    p.color[3] = if shown { alpha.max(0.0) } else { 0.0 };
    if spark.trail && age < life / 2 && (age + life).is_multiple_of(2) {
        let mut copy = Particle::new(p.pos, p.texture);
        copy.style = Kind::FireworkSpark;
        copy.friction = 0.91;
        copy.gravity = 0.1;
        copy.emissive = true;
        copy.size = p.size;
        copy.lifetime = p.lifetime;
        copy.age = p.lifetime / 2;
        copy.color = [p.color[0], p.color[1], p.color[2], 0.99];
        copy.spark = Some(Spark { trail: false, ..spark });
        return Some(copy);
    }
    None
}
