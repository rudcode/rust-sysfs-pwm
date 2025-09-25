use std::str::FromStr;

use tokio::fs;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncReadExt;
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

/// Open the specified entry name as a writable file
async fn pwm_file_wo(chip: &PwmChipAsync, pin: u32, name: &str) -> Result<File> {
    let f = OpenOptions::new()
        .write(true)
        .open(format!(
            "/sys/class/pwm/pwmchip{}/pwm{}/{}",
            chip.number, pin, name
        ))
        .await?;
    Ok(f)
}

/// Open the specified entry name as a readable file
async fn pwm_file_ro(chip: &PwmChipAsync, pin: u32, name: &str) -> Result<File> {
    let f = File::open(format!(
        "/sys/class/pwm/pwmchip{}/pwm{}/{}",
        chip.number, pin, name
    ))
    .await?;
    Ok(f)
}

/// Get the u32 value from the given entry
async fn pwm_file_parse<T: FromStr>(chip: &PwmChipAsync, pin: u32, name: &str) -> Result<T> {
    let mut s = String::with_capacity(10);
    let mut f = pwm_file_ro(chip, pin, name).await?;
    f.read_to_string(&mut s).await?;
    match s.trim().parse::<T>() {
        Ok(r) => Ok(r),
        Err(_) => Err(Error::Unexpected(format!(
            "Unexpeted value file contents: {:?}",
            s
        ))),
    }
}

/// Get the two u32 from capture file descriptor
async fn pwm_capture_parse<T: FromStr>(
    chip: &PwmChipAsync,
    pin: u32,
    name: &str,
) -> Result<Vec<T>> {
    let mut s = String::with_capacity(10);
    let mut f = pwm_file_ro(chip, pin, name).await?;
    f.read_to_string(&mut s).await?;
    s = s.trim().to_string();
    let capture = s.split_whitespace().collect::<Vec<_>>();
    let mut vec: Vec<T> = vec![];
    for s in capture.iter() {
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
        let npwm_path = format!("/sys/class/pwm/pwmchip{}/npwm", self.number);
        let mut npwm_file = File::open(&npwm_path).await?;
        let mut s = String::new();
        npwm_file.read_to_string(&mut s).await?;
        match s.parse::<u32>() {
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
            let path = format!("/sys/class/pwm/pwmchip{}/export", self.number);
            let mut export_file = File::create(&path).await?;
            let _ = export_file
                .write_all(format!("{}", number).as_bytes())
                .await;
            let _ = export_file.sync_all().await;
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
            let path = format!("/sys/class/pwm/pwmchip{}/unexport", self.number);
            let mut export_file = File::create(&path).await?;
            let _ = export_file
                .write_all(format!("{}", number).as_bytes())
                .await;
            let _ = export_file.sync_all().await;
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
        let mut enable_file = pwm_file_wo(&self.chip, self.number, "enable").await?;
        let contents = if enable { "1" } else { "0" };
        enable_file.write_all(contents.as_bytes()).await?;
        let _ = enable_file.sync_all().await;
        Ok(())
    }

    /// Query the state of enable for a given PWM pin
    pub async fn get_enabled(&self) -> Result<bool> {
        pwm_file_parse::<u32>(&self.chip, self.number, "enable")
            .await
            .map(|enable_state| match enable_state {
                1 => true,
                0 => false,
                _ => panic!("enable != 1|0 should be unreachable"),
            })
    }

    /// Get the currently configured duty_cycle in nanoseconds
    pub async fn get_duty_cycle_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(&self.chip, self.number, "duty_cycle").await
    }

    /// Get the capture
    pub async fn get_capture(&self) -> Result<(u32, u32)> {
        let t = pwm_capture_parse::<u32>(&self.chip, self.number, "capture").await?;
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
        // we'll just let the kernel do the validation
        let mut duty_cycle_file = pwm_file_wo(&self.chip, self.number, "duty_cycle").await?;
        duty_cycle_file
            .write_all(format!("{}", duty_cycle_ns).as_bytes())
            .await?;
        let _ = duty_cycle_file.sync_all().await;
        Ok(())
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
            .await?;
        Ok(())
    }

    /// Get the currently configured period in nanoseconds
    pub async fn get_period_ns(&self) -> Result<u32> {
        pwm_file_parse::<u32>(&self.chip, self.number, "period").await
    }

    /// The period of the PWM signal in Nanoseconds
    pub async fn set_period_ns(&self, period_ns: u32) -> Result<()> {
        let mut period_file = pwm_file_wo(&self.chip, self.number, "period").await?;
        period_file
            .write_all(format!("{}", period_ns).as_bytes())
            .await?;
        let _ = period_file.sync_all().await;
        Ok(())
    }

    /// Set the polarity of the PWM signal
    pub async fn set_polarity(&self, polarity: Polarity) -> Result<()> {
        let mut polarity_file = pwm_file_wo(&self.chip, self.number, "polarity").await?;
        match polarity {
            Polarity::Normal => polarity_file.write_all("normal".as_bytes()).await?,
            Polarity::Inverse => polarity_file.write_all("inversed".as_bytes()).await?,
        };
        Ok(())
    }

    /// Get the polarity of the PWM signal
    pub async fn get_polarity(&self) -> Result<Polarity> {
        let mut polarity_file = pwm_file_ro(&self.chip, self.number, "polarity").await?;
        let mut s = String::new();
        polarity_file.read_to_string(&mut s).await?;
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
