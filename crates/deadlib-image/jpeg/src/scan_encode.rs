use crate::progressive::ProgressiveScan;
use crate::{ComponentSpec, Error, Result};

/// One output scan. Component selectors are frame component IDs in frame order.
/// The sequence of separate component scans can use any component order.
/// The unused tail of `components` is ignored. Sequential parameters are
/// Ss=0, Se=63, Ah=Al=0; progressive parameters follow T.81 Annex G.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanSpec {
    pub components: [u8; 4],
    pub component_count: u8,
    pub spectral_start: u8,
    pub spectral_end: u8,
    pub successive_high: u8,
    pub successive_low: u8,
}
impl ScanSpec {
    /// Copy 1..4 selectors. Encoding validates parameters and frame/scan order.
    pub fn new(components: &[u8], ss: u8, se: u8, ah: u8, al: u8) -> Result<Self> {
        if !(1..=4).contains(&components.len()) {
            return Err(Error::Invalid("encoder scan component count"));
        }
        let mut ids = [0; 4];
        ids[..components.len()].copy_from_slice(components);
        Ok(Self {
            components: ids,
            component_count: components.len() as u8,
            spectral_start: ss,
            spectral_end: se,
            successive_high: ah,
            successive_low: al,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Scan {
    pub indices: [usize; 4],
    pub count: usize,
    pub progression: Option<ProgressiveScan>,
}
impl Scan {
    pub fn has_dc(self) -> bool {
        self.progression.is_none_or(|s| s.ss == 0)
    }
    pub fn has_ac(self) -> bool {
        self.progression.is_none_or(|s| s.ss != 0)
    }
}

/// Resolve on demand: even long caller scripts need no encoder-owned scan Vec.
pub(crate) struct Plan<'a> {
    components: &'a [ComponentSpec],
    progressive: bool,
    initial: u8,
    script: &'a [ScanSpec],
    boundaries: [u8; 256],
    groups: usize,
}
impl<'a> Plan<'a> {
    pub fn new(
        components: &'a [ComponentSpec],
        progressive: bool,
        initial: u8,
        script: &'a [ScanSpec],
        max_scans: usize,
    ) -> Result<Self> {
        if components.is_empty() || components.len() > 255 || (progressive && components.len() > 4)
        {
            return Err(Error::Invalid("encoder frame component count"));
        }
        if progressive && script.is_empty() && initial > 13 {
            return Err(Error::Invalid("encoder initial approximation"));
        }
        let mut plan = Self {
            components,
            progressive,
            initial,
            script,
            boundaries: [0; 256],
            groups: 0,
        };
        // Consecutive groups preserve frame order and avoid owned scan lists.
        // A component with more than ten sampling units is non-interleaved.
        if script.is_empty() {
            let mut start = 0;
            while start < components.len() {
                let mut end = start + 1;
                let mut blocks = components[start].horizontal_sampling as usize
                    * components[start].vertical_sampling as usize;
                while end < components.len() && end - start < 4 {
                    let next = components[end].horizontal_sampling as usize
                        * components[end].vertical_sampling as usize;
                    if blocks + next > 10 {
                        break;
                    }
                    blocks += next;
                    end += 1;
                }
                plan.boundaries[plan.groups] = start as u8;
                plan.groups += 1;
                start = end;
            }
            plan.boundaries[plan.groups] = components.len() as u8;
        }
        if plan.len() > max_scans {
            return Err(Error::LimitExceeded("encoder scans"));
        }
        let mut precision = [[-1; 64]; 4];
        let mut seen = [false; 255];
        for index in 0..plan.len() {
            let scan = plan.get(index)?;
            let mut blocks = 0;
            for (position, &i) in scan.indices[..scan.count].iter().enumerate() {
                if scan.indices[..position].contains(&i) {
                    return Err(Error::Invalid("duplicate encoder scan component"));
                }
                if position != 0 && i < scan.indices[position - 1] {
                    return Err(Error::Invalid("encoder scan component order"));
                }
                let c = components[i];
                blocks += c.horizontal_sampling as usize * c.vertical_sampling as usize;
                if let Some(progression) = scan.progression {
                    progression.advance(&mut precision[i])?;
                    if progression.ss == 0 {
                        seen[i] = true;
                    }
                } else {
                    if seen[i] {
                        return Err(Error::Invalid("duplicate sequential encoder component"));
                    }
                    seen[i] = true;
                }
            }
            if scan.count > 1 && blocks > 10 {
                return Err(Error::Invalid("encoder MCU exceeds ten blocks"));
            }
        }
        if seen[..components.len()].iter().any(|&v| !v) {
            return Err(Error::Invalid("encoder scan script misses a component"));
        }
        Ok(plan)
    }
    pub fn len(&self) -> usize {
        if !self.script.is_empty() {
            self.script.len()
        } else if self.progressive {
            (self.initial as usize + 1) * (self.components.len() + self.groups)
        } else {
            self.groups
        }
    }
    /// Realized band precision after the requested script. A pyramid must use
    /// these transmitted values, including unseen bands, as its next reference.
    pub fn progressive_precision(&self) -> Result<[[i8; 64]; 4]> {
        if !self.progressive {
            return Err(Error::Invalid("progressive precision for sequential frame"));
        }
        let mut precision = [[-1; 64]; 4];
        for index in 0..self.len() {
            let scan = self.get(index)?;
            for &i in &scan.indices[..scan.count] {
                if let Some(progression) = scan.progression {
                    progression.advance(&mut precision[i])?;
                } else {
                    precision[i].fill(0);
                }
            }
        }
        Ok(precision)
    }
    pub fn get(&self, index: usize) -> Result<Scan> {
        if !self.script.is_empty() {
            let spec = self.script[index];
            let count = spec.component_count as usize;
            if !(1..=4).contains(&count) {
                return Err(Error::Invalid("encoder scan component count"));
            }
            let mut indices = [0; 4];
            for (i, &id) in spec.components[..count].iter().enumerate() {
                indices[i] = self
                    .components
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or(Error::Invalid("encoder scan component ID"))?;
            }
            let progression = if self.progressive {
                if spec.successive_high > 13 || spec.successive_low > 13 {
                    return Err(Error::Invalid("progressive scan parameters"));
                }
                Some(ProgressiveScan::parse(
                    &[
                        spec.spectral_start,
                        spec.spectral_end,
                        (spec.successive_high << 4) | spec.successive_low,
                    ],
                    count,
                )?)
            } else {
                if [
                    spec.spectral_start,
                    spec.spectral_end,
                    spec.successive_high,
                    spec.successive_low,
                ] != [0, 63, 0, 0]
                {
                    return Err(Error::Invalid("sequential encoder scan parameters"));
                }
                None
            };
            return Ok(Scan {
                indices,
                count,
                progression,
            });
        }
        let mut indices = [0; 4];
        let count;
        let progression = if self.progressive {
            let stride = self.components.len() + self.groups;
            let level = index / stride;
            let band = index % stride;
            let al = self.initial - level as u8;
            if band >= self.groups {
                indices[0] = band - self.groups;
                count = 1;
            } else {
                count = self.group(band, &mut indices);
            }
            Some(ProgressiveScan {
                ss: usize::from(band >= self.groups),
                se: if band < self.groups { 0 } else { 63 },
                ah: if level == 0 { 0 } else { al + 1 },
                al,
            })
        } else {
            count = self.group(index, &mut indices);
            None
        };
        Ok(Scan {
            indices,
            count,
            progression,
        })
    }
    fn group(&self, index: usize, indices: &mut [usize; 4]) -> usize {
        let start = self.boundaries[index] as usize;
        let count = self.boundaries[index + 1] as usize - start;
        for (j, i) in indices[..count].iter_mut().enumerate() {
            *i = start + j;
        }
        count
    }
}
