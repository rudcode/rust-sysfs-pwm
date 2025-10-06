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

//! PWM access under Linux using the PWM sysfs interface

use std::fs;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;

use crate::common;
use common::{Error, Polarity, Result};

#[derive(Debug)]
pub struct PwmChip {
    pub number: u32,
}

#[derive(Debug)]
pub struct Pwm {
    chip: PwmChip,
    number: u32,
}

#[inline]
fn pwm_file_write(chip: u32, pin: u32, name: &str, value: &[u8]) -> Result<()> {
    Ok(File::create(format!("/sys/class/pwm/pwmchip{chip}/pwm{pin}/{name}"))?.write_all(value)?)
}

#[inline]
fn pwm_file_read(chip: u32, pin: u32, name: &str) -> Result<String> {
    Ok(fs::read_to_string(format!(
        "/sys/class/pwm/pwmchip{chip}/pwm{pin}/{name}"
    ))?)
}

#[inline]
fn pwm_file_parse<T: FromStr>(chip: u32, pin: u32, name: &str) -> Result<T> {
    let s = pwm_file_read(chip, pin, name)?;
    match s.trim().parse::<T>() {
        Ok(r) => Ok(r),
        Err(_) => Err(Error::Unexpected(format!(
            "Unexpeted value file contents: {:?}",
            s
        ))),
    }
}

#[inline]
fn pwm_file_parse_vec<T: FromStr>(chip: u32, pin: u32, name: &str) -> Result<Vec<T>> {
    let s = pwm_file_read(chip, pin, name)?.trim().to_string();
    let vec_s = s.split_whitespace().collect::<Vec<_>>();
    let mut vec: Vec<T> = vec![];
    for s in vec_s.iter() {
        if let Ok(j) = s.parse::<T>() {
            vec.push(j);
        }
    }
    Ok(vec)
}

impl PwmChip {
    pub fn new(number: u32) -> Result<PwmChip> {
        fs::metadata(&format!("/sys/class/pwm/pwmchip{}", number))?;
        Ok(PwmChip { number })
    }

    pub fn count(&self) -> Result<u32> {
        let s = fs::read_to_string(format!("/sys/class/pwm/pwmchip{}/npwm", self.number))?;
        match s.trim().parse::<u32>() {
            Ok(n) => Ok(n),
            Err(_) => Err(Error::Unexpected(format!(
                "Unexpected npwm contents: {:?}",
                s
            ))),
        }
    }

    pub fn export(&self, number: u32) -> Result<()> {
        // only export if not already exported
        if fs::metadata(&format!(
            "/sys/class/pwm/pwmchip{}/pwm{}",
            self.number, number
        ))
        .is_err()
        {
            File::create(format!("/sys/class/pwm/pwmchip{}/export", self.number))?
                .write_all(number.to_string().as_bytes())?;
        }
        Ok(())
    }

    pub fn unexport(&self, number: u32) -> Result<()> {
        if fs::metadata(&format!(
            "/sys/class/pwm/pwmchip{}/pwm{}",
            self.number, number
        ))
        .is_ok()
        {
            File::create(format!("/sys/class/pwm/pwmchip{}/unexport", self.number))?
                .write_all(number.to_string().as_bytes())?;
        }
        Ok(())
    }
}

impl Pwm {
    /// Create a new Pwm with the provided chip/number
    ///
    /// This function does not export the Pwm pin
    pub fn new(chip: u32, number: u32) -> Result<Pwm> {
        let chip: PwmChip = PwmChip::new(chip)?;
        Ok(Pwm { chip, number })
    }

    /// Run a closure with the GPIO exported
    #[inline]
    pub fn with_exported<F>(&self, closure: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.export()?;
        match closure() {
            Ok(()) => self.unexport(),
            Err(e) => match self.unexport() {
                Ok(()) => Err(e),
                Err(ue) => Err(Error::Unexpected(format!(
                    "Failed unexporting due to:\n{}\nwhile handling:\n{}",
                    ue, e
                ))),
            },
        }
    }

    /// Export the Pwm for use
    pub fn export(&self) -> Result<()> {
        self.chip.export(self.number)
    }

    /// Unexport the PWM
    pub fn unexport(&self) -> Result<()> {
        self.chip.unexport(self.number)
    }

    /// Enable/Disable the PWM Signal
    pub fn enable(&self, enable: bool) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "enable",
            &(enable as u8).to_string().as_bytes(),
        )
    }

    /// Query the state of enable for a given PWM pin
    pub fn get_enabled(&self) -> Result<bool> {
        Ok(
            match pwm_file_read(self.chip.number, self.number, "enable")?.trim() {
                "1" => true,
                "0" => false,
                _ => panic!("enable != 1|0 should be unreachable"),
            },
        )
    }

    /// Get the currently configured duty_cycle in nanoseconds
    pub fn get_duty_cycle_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(self.chip.number, self.number, "duty_cycle")
    }

    /// Get the capture
    pub fn get_capture(&self) -> Result<(u32, u32)> {
        let t = pwm_file_parse_vec::<u32>(self.chip.number, self.number, "capture")?;
        if t.len() == 2 {
            Ok((t[0], t[1]))
        } else {
            Err(Error::Unexpected(format!("Failed exporting")))
        }
    }

    /// The active time of the PWM signal
    ///
    /// Value is in nanoseconds and must be less than the period.
    pub fn set_duty_cycle_ns(&self, duty_cycle_ns: u32) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "duty_cycle",
            &duty_cycle_ns.to_string().as_bytes(),
        )
    }

    /// Get the currently configured duty_cycle as percentage of period
    pub fn get_duty_cycle(&self) -> Result<f32> {
        Ok((self.get_duty_cycle_ns()? as f32) / (self.get_period_ns()? as f32))
    }

    /// The active time of the PWM signal
    ///
    /// Value is as percentage of period.
    pub fn set_duty_cycle(&self, duty_cycle: f32) -> Result<()> {
        self.set_duty_cycle_ns((self.get_period_ns()? as f32 * duty_cycle).round() as u32)
    }

    /// Get the currently configured period in nanoseconds
    pub fn get_period_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(self.chip.number, self.number, "period")
    }

    /// The period of the PWM signal in Nanoseconds
    pub fn set_period_ns(&self, period_ns: u32) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "period",
            &period_ns.to_string().as_bytes(),
        )
    }

    /// Set the polarity of the PWM signal
    pub fn set_polarity(&self, polarity: Polarity) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "polarity",
            match polarity {
                Polarity::Normal => b"normal",
                Polarity::Inverse => b"inversed",
            },
        )
    }

    /// Get the polarity of the PWM signal
    pub fn get_polarity(&self) -> Result<Polarity> {
        let s = pwm_file_read(self.chip.number, self.number, "polarity")?;
        match s.trim() {
            "normal" => Ok(Polarity::Normal),
            "inversed" => Ok(Polarity::Inverse),
            _ => Err(Error::Unexpected(format!(
                "Unexpected polarity file contents: {:?}",
                s
            ))),
        }
    }
}
