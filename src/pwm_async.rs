use std::str::FromStr;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::common;
use common::{Error, Polarity, Result};

#[derive(Debug)]
pub struct PwmAsync {
    chip: PwmChipAsync,
    number: u32,
}

#[derive(Debug)]
pub struct PwmChipAsync {
    pub number: u32,
}

#[inline]
async fn pwm_file_write(chip: u32, pin: u32, name: &str, value: &[u8]) -> Result<()> {
    Ok(
        File::create(format!("/sys/class/pwm/pwmchip{chip}/pwm{pin}/{name}"))
            .await?
            .write_all(value)
            .await?,
    )
}

#[inline]
async fn pwm_file_read(chip: u32, pin: u32, name: &str) -> Result<String> {
    Ok(fs::read_to_string(format!("/sys/class/pwm/pwmchip{chip}/pwm{pin}/{name}")).await?)
}

#[inline]
async fn pwm_file_parse<T: FromStr>(chip: u32, pin: u32, name: &str) -> Result<T> {
    let s = pwm_file_read(chip, pin, name).await?;
    match s.trim().parse::<T>() {
        Ok(r) => Ok(r),
        Err(_) => Err(Error::Unexpected(format!(
            "Unexpeted value file contents: {:?}",
            s
        ))),
    }
}

#[inline]
async fn pwm_file_parse_vec<T: FromStr>(chip: u32, pin: u32, name: &str) -> Result<Vec<T>> {
    let s = pwm_file_read(chip, pin, name).await?.trim().to_string();
    let vec_s = s.split_whitespace().collect::<Vec<_>>();
    let mut vec: Vec<T> = vec![];
    for s in vec_s.iter() {
        if let Ok(j) = s.parse::<T>() {
            vec.push(j);
        }
    }
    Ok(vec)
}

impl PwmChipAsync {
    pub async fn new(number: u32) -> Result<PwmChipAsync> {
        fs::metadata(&format!("/sys/class/pwm/pwmchip{}", number)).await?;
        Ok(PwmChipAsync { number: number })
    }

    pub async fn count(&self) -> Result<u32> {
        let s = fs::read_to_string(format!("/sys/class/pwm/pwmchip{}/npwm", self.number)).await?;
        match s.trim().parse::<u32>() {
            Ok(n) => Ok(n),
            Err(_) => Err(Error::Unexpected(format!(
                "Unexpected npwm contents: {:?}",
                s
            ))),
        }
    }

    pub async fn export(&self, number: u32) -> Result<()> {
        // only export if not already exported
        if fs::metadata(&format!(
            "/sys/class/pwm/pwmchip{}/pwm{}",
            self.number, number
        ))
        .await
        .is_err()
        {
            File::create(format!("/sys/class/pwm/pwmchip{}/export", self.number))
                .await?
                .write_all(number.to_string().as_bytes())
                .await?;
        }
        Ok(())
    }

    pub async fn unexport(&self, number: u32) -> Result<()> {
        if fs::metadata(&format!(
            "/sys/class/pwm/pwmchip{}/pwm{}",
            self.number, number
        ))
        .await
        .is_ok()
        {
            File::create(format!("/sys/class/pwm/pwmchip{}/unexport", self.number))
                .await?
                .write_all(number.to_string().as_bytes())
                .await?;
        }
        Ok(())
    }
}
impl PwmAsync {
    /// Create a new Pwm with the provided chip/number
    ///
    /// This function does not export the Pwm pin
    pub async fn new(chip: u32, number: u32) -> Result<PwmAsync> {
        let chip: PwmChipAsync = PwmChipAsync::new(chip).await?;
        Ok(PwmAsync {
            chip: chip,
            number: number,
        })
    }

    /// Run a closure with the GPIO exported
    #[inline]
    pub async fn with_exported<F>(&self, closure: F) -> Result<()>
    where
        F: AsyncFnOnce() -> Result<()>,
    {
        self.export().await?;
        let y = closure().await;
        match y {
            Ok(()) => self.unexport().await,
            Err(e) => match self.unexport().await {
                Ok(()) => Err(e),
                Err(ue) => Err(Error::Unexpected(format!(
                    "Failed unexporting due to:\n{}\nwhile handling:\n{}",
                    ue, e
                ))),
            },
        }
    }

    /// Export the Pwm for use
    pub async fn export(&self) -> Result<()> {
        self.chip.export(self.number).await
    }

    /// Unexport the PWM
    pub async fn unexport(&self) -> Result<()> {
        self.chip.unexport(self.number).await
    }

    /// Enable/Disable the PWM Signal
    pub async fn enable(&self, enable: bool) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "enable",
            &(enable as u8).to_string().as_bytes(),
        )
        .await
    }

    /// Query the state of enable for a given PWM pin
    pub async fn get_enabled(&self) -> Result<bool> {
        Ok(
            match pwm_file_read(self.chip.number, self.number, "enable")
                .await?
                .trim()
            {
                "1" => true,
                "0" => false,
                _ => panic!("enable != 1|0 should be unreachable"),
            },
        )
    }

    /// Get the currently configured duty_cycle in nanoseconds
    pub async fn get_duty_cycle_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(self.chip.number, self.number, "duty_cycle").await
    }

    /// Get the capture
    pub async fn get_capture(&self) -> Result<(u32, u32)> {
        let t = pwm_file_parse_vec::<u32>(self.chip.number, self.number, "capture").await?;
        if t.len() == 2 {
            Ok((t[0], t[1]))
        } else {
            Err(Error::Unexpected(format!("Failed exporting")))
        }
    }

    /// The active time of the PWM signal
    ///
    /// Value is in nanoseconds and must be less than the period.
    pub async fn set_duty_cycle_ns(&self, duty_cycle_ns: u32) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "duty_cycle",
            &duty_cycle_ns.to_string().as_bytes(),
        )
        .await
    }

    /// Get the currently configured duty_cycle as percentage of period
    pub async fn get_duty_cycle(&self) -> Result<f32> {
        Ok((self.get_duty_cycle_ns().await? as f32) / (self.get_period_ns().await? as f32))
    }

    /// The active time of the PWM signal
    ///
    /// Value is as percentage of period.
    pub async fn set_duty_cycle(&self, duty_cycle: f32) -> Result<()> {
        self.set_duty_cycle_ns((self.get_period_ns().await? as f32 * duty_cycle).round() as u32)
            .await
    }

    /// Get the currently configured period in nanoseconds
    pub async fn get_period_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(self.chip.number, self.number, "period").await
    }

    /// The period of the PWM signal in Nanoseconds
    pub async fn set_period_ns(&self, period_ns: u32) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "period",
            &period_ns.to_string().as_bytes(),
        )
        .await
    }

    /// Set the polarity of the PWM signal
    pub async fn set_polarity(&self, polarity: Polarity) -> Result<()> {
        pwm_file_write(
            self.chip.number,
            self.number,
            "polarity",
            match polarity {
                Polarity::Normal => b"normal",
                Polarity::Inverse => b"inversed",
            },
        )
        .await
    }

    /// Get the polarity of the PWM signal
    pub async fn get_polarity(&self) -> Result<Polarity> {
        let s = pwm_file_read(self.chip.number, self.number, "polarity").await?;
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
