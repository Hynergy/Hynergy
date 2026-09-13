pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
    base_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Truncated {
    pub offset: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(input: &'a [u8], base_offset: usize) -> Self {
        Self {
            input,
            position: 0,
            base_offset,
        }
    }

    #[inline]
    pub(crate) const fn offset(&self) -> usize {
        self.base_offset + self.position
    }

    #[inline]
    pub(crate) const fn remaining(&self) -> usize {
        self.input.len() - self.position
    }

    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, Truncated> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, Truncated> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, Truncated> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_f64(&mut self) -> Result<f64, Truncated> {
        Ok(f64::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_array<const N: usize>(&mut self) -> Result<[u8; N], Truncated> {
        let bytes = self.read_bytes(N)?;

        Ok(bytes.try_into().expect("slice length was checked"))
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], Truncated> {
        let Some(end) = self.position.checked_add(length) else {
            return Err(self.truncated());
        };

        if end > self.input.len() {
            return Err(self.truncated());
        }

        let bytes = &self.input[self.position..end];
        self.position = end;

        Ok(bytes)
    }

    #[inline]
    const fn truncated(&self) -> Truncated {
        Truncated {
            offset: self.base_offset + self.input.len(),
        }
    }
}
