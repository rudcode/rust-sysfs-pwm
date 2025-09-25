// Copyright 2016, Paul Osborne <osbpau@gmail.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/license/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option.  This file may not be copied, modified, or distributed
// except according to those terms.
//
// Portions of this implementation are based on work by Nat Pryce:
// https://github.com/npryce/rusty-pi/blob/master/src/pi/gpio.rs

extern crate sysfs_pwm;
extern crate tokio;
use sysfs_pwm::common::Result;
use sysfs_pwm::pwm_async::PwmAsync;

// PIN: EHRPWM0A (P9_22)
const BB_PWM_CHIP: u32 = 0;
const BB_PWM_NUMBER: u32 = 0;

async fn pwm_increase_to_max(
    pwm: &PwmAsync,
    duration_ms: u32,
    update_period_ms: u32,
) -> Result<()> {
    let step: f32 = duration_ms as f32 / update_period_ms as f32;
    let mut duty_cycle = 0.0;
    let period_ns: u32 = pwm.get_period_ns().await?;
    while duty_cycle < 1.0 {
        pwm.set_duty_cycle_ns((duty_cycle * period_ns as f32) as u32)
            .await?;
        duty_cycle += step;
    }
    pwm.set_duty_cycle_ns(period_ns).await
}

async fn pwm_decrease_to_minimum(
    pwm: &PwmAsync,
    duration_ms: u32,
    update_period_ms: u32,
) -> Result<()> {
    let step: f32 = duration_ms as f32 / update_period_ms as f32;
    let mut duty_cycle = 1.0;
    let period_ns: u32 = pwm.get_period_ns().await?;
    while duty_cycle > 0.0 {
        pwm.set_duty_cycle_ns((duty_cycle * period_ns as f32) as u32)
            .await?;
        duty_cycle -= step;
    }
    pwm.set_duty_cycle_ns(0).await
}

/// Make an LED "breathe" by increasing and
/// decreasing the brightness
#[tokio::main]
async fn main() {
    let pwm_async = PwmAsync::new(BB_PWM_CHIP, BB_PWM_NUMBER).await.unwrap(); // number depends on chip, etc.
    pwm_async
        .with_exported(|| async {
            pwm_async.enable(true).await.unwrap();
            pwm_async.set_period_ns(20_000).await.unwrap();
            loop {
                pwm_increase_to_max(&pwm_async, 1000, 20).await.unwrap();
                pwm_decrease_to_minimum(&pwm_async, 1000, 20).await.unwrap();
            }
        })
        .await
        .unwrap();
}
