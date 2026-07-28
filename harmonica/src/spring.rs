pub struct Spring {
    pos_pos_coef: f32,
    pos_vel_coef: f32,
    vel_pos_coef: f32,
    vel_vel_coef: f32,
    pos: f32,
    vel: f32,
}

impl Spring {
    pub fn new(
        delta_time: f32,
        angular_frequency: f32,
        damping_ratio: f32,
        initial_position: f32,
        initial_velocity: f32,
    ) -> Self {
        // Keep values in a legal range.
        let angular_frequency = angular_frequency.max(0.0);
        let damping_ratio = damping_ratio.max(0.0);

        // If there is no angular frequency, the spring will not move and we can
        // return identity.
        if angular_frequency < f32::EPSILON {
            Self {
                pos_pos_coef: 1.0,
                pos_vel_coef: 0.0,
                vel_pos_coef: 0.0,
                vel_vel_coef: 1.0,
                pos: initial_position,
                vel: initial_velocity,
            }
        } else if damping_ratio > 1.0 + f32::EPSILON {
            // Over-damped.
            let za = -angular_frequency * damping_ratio;
            let zb = angular_frequency * (damping_ratio * damping_ratio - 1.0).sqrt();

            let z1 = za - zb;
            let z2 = za + zb;

            let e1 = (z1 * delta_time).exp();
            let e2 = (z2 * delta_time).exp();

            let inv_two_zb = 1.0 / (2.0 * zb); // = 1 / (z2 - z1)

            let e1_over_two_zb = e1 * inv_two_zb;
            let e2_over_two_zb = e2 * inv_two_zb;

            let z1_e1_over_two_zb = z1 * e1_over_two_zb;
            let z2_e2_over_two_zb = z2 * e2_over_two_zb;

            Self {
                pos_pos_coef: e1_over_two_zb * z2 - z2_e2_over_two_zb + e2,
                pos_vel_coef: -e1_over_two_zb + e2_over_two_zb,
                vel_pos_coef: (z1_e1_over_two_zb - z2_e2_over_two_zb + e2) * z2,
                vel_vel_coef: -z1_e1_over_two_zb + z2_e2_over_two_zb,
                pos: initial_position,
                vel: initial_velocity,
            }
        } else if damping_ratio < 1.0 - f32::EPSILON {
            // Under-damped.
            let omega_zeta = angular_frequency * damping_ratio;
            let alpha = angular_frequency * (1.0 - damping_ratio * damping_ratio).sqrt();

            let exp_term = (-omega_zeta * delta_time).exp();
            let cos_term = (alpha * delta_time).cos();
            let sin_term = (alpha * delta_time).sin();

            let inv_alpha = 1.0 / alpha;

            let exp_sin = exp_term * sin_term;
            let exp_cos = exp_term * cos_term;
            let exp_omega_zeta_sin_over_alpha = exp_term * omega_zeta * sin_term * inv_alpha;

            Self {
                pos_pos_coef: exp_cos + exp_omega_zeta_sin_over_alpha,
                pos_vel_coef: exp_sin * inv_alpha,
                vel_pos_coef: -exp_sin * alpha - omega_zeta * exp_omega_zeta_sin_over_alpha,
                vel_vel_coef: exp_cos - exp_omega_zeta_sin_over_alpha,
                pos: initial_position,
                vel: initial_velocity,
            }
        } else {
            // Critically damped.
            let exp_term = (-angular_frequency * delta_time).exp();
            let time_exp = delta_time * exp_term;
            let time_exp_freq = time_exp * angular_frequency;

            Self {
                pos_pos_coef: time_exp_freq + exp_term,
                pos_vel_coef: time_exp,
                vel_pos_coef: -angular_frequency * time_exp_freq,
                vel_vel_coef: -time_exp_freq + exp_term,
                pos: initial_position,
                vel: initial_velocity,
            }
        }
    }

    pub fn update(&mut self, equilibrium_pos: f32) -> (f32, f32) {
        let old_pos = self.pos - equilibrium_pos; // update in equilibrium relative space
        let old_vel = self.vel;

        self.pos = old_pos * self.pos_pos_coef + old_vel * self.pos_vel_coef + equilibrium_pos;
        self.vel = old_pos * self.vel_pos_coef + old_vel * self.vel_vel_coef;

        (self.pos, self.vel)
    }
}
