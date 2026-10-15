use core::fmt::{self, Formatter};

#[derive(Debug, Copy, Clone, PartialEq, Eq, defmt::Format)]
pub enum HallError {
    /// Invalid configuration parameter (e.g., N = 0)
    InvalidConfig,
    /// Too many outlier pulses rejected during median filtering
    ExcessiveNoise,
}

// core::error::Error is stable in core since Rust 1.81
impl core::error::Error for HallError {}

impl fmt::Display for HallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HallError::InvalidConfig => write!(f, "Invalid configuration parameter N"),
            HallError::ExcessiveNoise => write!(f, "Too many outliers rejected, unreliable RPM"),
        }
    }
}

pub fn extract_rpm<const N: usize>(mut time_hall_low: [u64; N]) -> Result<u16, HallError> {
    if N <= 2 {
        return Err(HallError::InvalidConfig);
    }
    //Compare the each value to the median, if extraneous (~2x different) then "remove"
    time_hall_low.sort_unstable();
    let mut valid_length = N; //Keep track of how many items get removed
    let median = time_hall_low[N / 2];
    for index in 0..N {
        if time_hall_low[index] < median / 2 || time_hall_low[index] > 2 * median {
            //shift everything left
            for i in index..N - 1 {
                time_hall_low[i] = time_hall_low[i + 1];
            }
            valid_length -= 1;
        }
    }

    let threshold = N / 4; //any number of rejects greater than 25% throws error
    if N - valid_length >= threshold {
        return Err(HallError::ExcessiveNoise);
    }

    //take the average time difference in us from the valid values
    let avg_dif_us = time_hall_low[0..valid_length].iter().sum::<u64>() / (valid_length as u64);
    if avg_dif_us == 0 {
        return Err(HallError::ExcessiveNoise);
    }
    let rpm = 60000000 / (avg_dif_us);
    return Ok(rpm as u16);
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_rpm_from_clean_samples() {
        let samples = [100u64, 100, 100, 100, 100];

        let rpm = extract_rpm(samples).unwrap();
        dbg!(rpm);

        assert_eq!(rpm, 600);
    }

    #[test]
    fn rejects_outlier_and_calculates_rpm() {
        let samples = [100u64, 100, 100, 100, 500, 125, 110, 95];

        let rpm = extract_rpm(samples).unwrap();
        dbg!(rpm);

        assert_eq!(rpm, 576);
    }

    #[test]
    fn all_same_value() {
        let samples = [50u64; 8];

        let rpm = extract_rpm(samples).unwrap();
        dbg!(rpm);

        assert_eq!(rpm, 1200);
    }
    #[test]
    fn excessive_noise_returns_error() {
        let samples = [100u64, 100, 100, 100, 500, 600, 700, 800];

        let err = extract_rpm(samples).unwrap_err();

        assert_eq!(err, HallError::ExcessiveNoise);
    }

    #[test]
    fn invalid_config_returns_error() {
        // Requires:
        // if N == 0 {
        //     return Err(HallError::InvalidConfig);
        // }
        // at the start of extract_rpm()

        let err = extract_rpm::<0>([]).unwrap_err();

        assert_eq!(err, HallError::InvalidConfig);
    }

    #[test]
    fn display_messages_are_correct() {
        assert_eq!(
            HallError::InvalidConfig.to_string(),
            "Invalid configuration parameter N"
        );

        assert_eq!(
            HallError::ExcessiveNoise.to_string(),
            "Too many outliers rejected, unreliable RPM"
        );
    }

    #[test]
    fn hall_error_implements_error_trait() {
        fn assert_error<E: core::error::Error>() {}

        assert_error::<HallError>();
    }

    #[test]
    fn single_outlier_is_accepted() {
        let samples = [100u64, 100, 100, 100, 100, 100, 100, 500];

        let rpm = extract_rpm(samples).unwrap();

        assert_eq!(rpm, 600);
    }

    #[test]
    fn excessive_noise_at_threshold_returns_error() {
        let samples = [100u64, 100, 100, 100, 100, 500, 600, 100];

        let err = extract_rpm(samples).unwrap_err();

        assert_eq!(err, HallError::ExcessiveNoise);
    }
}
