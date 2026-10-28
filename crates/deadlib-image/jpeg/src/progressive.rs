use crate::entropy::{Bits, Huffman};
use crate::{Error, Result, ZIGZAG};

#[derive(Clone, Copy)]
pub(crate) struct ProgressiveScan {
    pub ss: usize,
    pub se: usize,
    pub ah: u8,
    pub al: u8,
}

impl ProgressiveScan {
    pub fn parse(params: &[u8], components: usize) -> Result<Self> {
        let ss = params[0] as usize;
        let se = params[1] as usize;
        let ah = params[2] >> 4;
        let al = params[2] & 15;
        if ss > se
            || se > 63
            || (ss == 0 && se != 0)
            || (ss != 0 && components != 1)
            || ah > 13
            || al > 13
            || (ah != 0 && ah != al + 1)
        {
            return Err(Error::Invalid("progressive scan parameters"));
        }
        Ok(Self { ss, se, ah, al })
    }

    pub fn advance(self, state: &mut [i8; 64]) -> Result<()> {
        if self.ss != 0 && state[0] < 0 {
            return Err(Error::Invalid("AC scan precedes DC"));
        }
        let expected = if self.ah == 0 { -1 } else { self.ah as i8 };
        if state[self.ss..=self.se].iter().any(|&v| v != expected) {
            return Err(Error::Invalid("progressive scan order"));
        }
        state[self.ss..=self.se].fill(self.al as i8);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decode(
        self,
        block: &mut [i16; 64],
        bits: &mut Bits<'_>,
        dc: Option<&Huffman>,
        ac: Option<&Huffman>,
        predictor: &mut i32,
        eob: &mut u32,
        precision: u8,
        differential: bool,
    ) -> Result<()> {
        if self.ss == 0 {
            if self.ah == 0 {
                let size = dc.unwrap().decode(bits)?;
                if size > precision + 3 {
                    return Err(Error::Invalid("progressive DC magnitude"));
                }
                *predictor += bits.receive(size)?;
                block[0] = i16::try_from(*predictor << self.al)
                    .map_err(|_| Error::Invalid("progressive DC range"))?;
            } else {
                // DC point transforms use arithmetic shifts: refinement adds
                // the actual two's-complement bit, including for negatives.
                block[0] |= (bits.read(1)? << self.al) as i16;
            }
            return Ok(());
        }
        if self.ah == 0 {
            self.ac_initial(
                block,
                bits,
                ac.unwrap(),
                eob,
                precision + u8::from(differential),
            )
        } else {
            self.ac_refine(block, bits, ac.unwrap(), eob)
        }
    }

    fn ac_initial(
        self,
        block: &mut [i16; 64],
        bits: &mut Bits<'_>,
        table: &Huffman,
        eob: &mut u32,
        precision: u8,
    ) -> Result<()> {
        if *eob > 0 {
            *eob -= 1;
            return Ok(());
        }
        let mut k = self.ss;
        while k <= self.se {
            let rs = table.decode(bits)?;
            let run = (rs >> 4) as usize;
            let size = rs & 15;
            if size == 0 {
                if run == 15 {
                    k += 16;
                    if k > self.se + 1 {
                        return Err(Error::Invalid("progressive zero run exceeds band"));
                    }
                } else {
                    *eob = (1 << run) + bits.read(run as u8)? - 1;
                    break;
                }
            } else {
                k += run;
                if k > self.se || size > precision + 2 {
                    return Err(Error::Invalid("progressive AC run or magnitude"));
                }
                block[ZIGZAG[k]] = i16::try_from(bits.receive(size)? << self.al)
                    .map_err(|_| Error::Invalid("progressive AC range"))?;
                k += 1;
            }
        }
        Ok(())
    }

    fn correction(self, value: &mut i16, bits: &mut Bits<'_>) -> Result<()> {
        let bit = 1i32 << self.al;
        if bits.read(1)? != 0 {
            let old = i32::from(*value);
            // A coefficient has exactly one correction at this bit position.
            if old.abs() & bit != 0 {
                return Err(Error::Invalid("duplicate AC correction"));
            }
            *value = i16::try_from(old + if old > 0 { bit } else { -bit })
                .map_err(|_| Error::Invalid("refined AC range"))?;
        }
        Ok(())
    }

    fn ac_refine(
        self,
        block: &mut [i16; 64],
        bits: &mut Bits<'_>,
        table: &Huffman,
        eob: &mut u32,
    ) -> Result<()> {
        let mut k = self.ss;
        if *eob == 0 {
            while k <= self.se {
                let rs = table.decode(bits)?;
                let run = (rs >> 4) as usize;
                let size = rs & 15;
                if size > 1 {
                    return Err(Error::Invalid("AC refinement magnitude"));
                }
                if size == 0 && run != 15 {
                    *eob = (1 << run) + bits.read(run as u8)?;
                    break;
                }
                let new_value = if size == 1 {
                    Some(if bits.read(1)? != 0 {
                        1i16 << self.al
                    } else {
                        -(1i16 << self.al)
                    })
                } else {
                    None
                };
                let mut zeros = if new_value.is_some() { run } else { 16 };
                let mut completed = false;
                while k <= self.se {
                    let value = &mut block[ZIGZAG[k]];
                    if *value != 0 {
                        self.correction(value, bits)?;
                        k += 1;
                    } else if zeros > 0 {
                        zeros -= 1;
                        k += 1;
                        if zeros == 0 && new_value.is_none() {
                            completed = true;
                            break;
                        }
                    } else {
                        *value = new_value.unwrap();
                        k += 1;
                        completed = true;
                        break;
                    }
                }
                if !completed {
                    return Err(Error::Invalid("AC refinement run exceeds band"));
                }
            }
        }
        if *eob > 0 {
            for &index in &ZIGZAG[k..=self.se] {
                if block[index] != 0 {
                    self.correction(&mut block[index], bits)?;
                }
            }
            *eob -= 1;
        }
        Ok(())
    }
}
