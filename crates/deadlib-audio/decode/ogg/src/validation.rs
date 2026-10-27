use crate::{Error, Page, UNKNOWN_GRANULE_POSITION};

/// Explicit compatibility exceptions. Both are disabled by default; CRC,
/// sequence, continuation, packet and physical-stream validation remain strict.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ValidationOptions {
    /// Ignore granule zero on a nonempty page that completes no packet. Some
    /// historical encoders incorrectly marked unfinished secondary headers zero.
    pub allow_zero_span_granules: bool,
    /// Permit a final EOS granule below an earlier page's granule. Only a codec
    /// chooses how to report or repair the inconsistent endpoint; this option
    /// alone neither retracts PCM nor determines its count.
    /// This never permits decreases on non-EOS pages.
    pub allow_decreasing_eos_granule: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityStats {
    pub zero_span_granules: u64,
    pub decreasing_eos_granules: u64,
}
impl CompatibilityStats {
    pub(crate) fn add(&mut self, value: Self) -> Result<(), Error> {
        self.zero_span_granules = self
            .zero_span_granules
            .checked_add(value.zero_span_granules)
            .ok_or(Error::LimitExceeded("compatibility event count"))?;
        self.decreasing_eos_granules = self
            .decreasing_eos_granules
            .checked_add(value.decreasing_eos_granules)
            .ok_or(Error::LimitExceeded("compatibility event count"))?;
        Ok(())
    }
}
pub(crate) fn granule(
    page: Page<'_>,
    previous: Option<u64>,
    options: ValidationOptions,
) -> Result<(Option<u64>, CompatibilityStats), Error> {
    let header = page.header();
    let gp = header.granule_position;
    let complete = page.completed_packets();
    let mut stats = CompatibilityStats::default();
    if complete == 0 && !page.lacing().is_empty() && gp != UNKNOWN_GRANULE_POSITION {
        if options.allow_zero_span_granules && gp == 0 {
            stats.zero_span_granules = 1;
            return Ok((None, stats));
        }
        return Err(Error::Invalid("granule on page without a completed packet"));
    }
    if gp == UNKNOWN_GRANULE_POSITION {
        if complete != 0 {
            return Err(Error::Invalid(
                "unknown granule on page with completed packets",
            ));
        }
        return Ok((None, stats));
    }
    if previous.is_some_and(|previous| gp < previous) {
        if !header.is_eos() || !options.allow_decreasing_eos_granule {
            return Err(Error::Invalid("decreasing granule position"));
        }
        stats.decreasing_eos_granules = 1;
    }
    Ok((Some(gp), stats))
}
