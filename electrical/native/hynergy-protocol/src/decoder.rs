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

#[cfg(test)]
mod tests {
    use super::{Decoder, Truncated};

    #[test]
    fn reads_little_endian_primitives_and_tracks_position() {
        let mut bytes = Vec::new();
        bytes.push(0x7f);
        bytes.extend_from_slice(&0x1234_u16.to_le_bytes());
        bytes.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        bytes.extend_from_slice(&3.5_f64.to_le_bytes());

        let mut decoder = Decoder::new(&bytes, 11);

        assert_eq!(decoder.offset(), 11);
        assert_eq!(decoder.remaining(), bytes.len());
        assert!(!decoder.is_empty());

        assert_eq!(decoder.read_u8().unwrap(), 0x7f);
        assert_eq!(decoder.offset(), 12);

        assert_eq!(decoder.read_u16().unwrap(), 0x1234);
        assert_eq!(decoder.offset(), 14);

        assert_eq!(decoder.read_u32().unwrap(), 0x1234_5678);
        assert_eq!(decoder.offset(), 18);

        assert_eq!(decoder.read_f64().unwrap(), 3.5);
        assert_eq!(decoder.offset(), 26);
        assert_eq!(decoder.remaining(), 0);
        assert!(decoder.is_empty());
    }

    #[test]
    fn read_bytes_returns_requested_ranges_and_advances() {
        let bytes = [1, 2, 3, 4, 5];
        let mut decoder = Decoder::new(&bytes, 8);

        assert_eq!(decoder.read_bytes(2).unwrap(), &[1, 2]);
        assert_eq!(decoder.offset(), 10);
        assert_eq!(decoder.remaining(), 3);

        assert_eq!(decoder.read_bytes(3).unwrap(), &[3, 4, 5]);
        assert_eq!(decoder.offset(), 13);
        assert!(decoder.is_empty());
    }

    #[test]
    fn read_array_reads_exact_number_of_bytes() {
        let bytes = [1, 2, 3, 4];
        let mut decoder = Decoder::new(&bytes, 0);

        assert_eq!(decoder.read_array::<4>().unwrap(), bytes);
        assert!(decoder.is_empty());
    }

    #[test]
    fn zero_length_read_does_not_advance() {
        let bytes = [1, 2, 3];
        let mut decoder = Decoder::new(&bytes, 17);

        assert_eq!(decoder.read_bytes(0).unwrap(), &[]);
        assert_eq!(decoder.offset(), 17);
        assert_eq!(decoder.remaining(), 3);
    }

    #[test]
    fn truncated_read_reports_absolute_end_offset() {
        let bytes = [1, 2, 3];
        let mut decoder = Decoder::new(&bytes, 20);

        assert_eq!(decoder.read_u32(), Err(Truncated { offset: 23 }));
    }

    #[test]
    fn truncated_read_does_not_advance_decoder() {
        let bytes = [1, 2, 3];
        let mut decoder = Decoder::new(&bytes, 20);

        assert!(decoder.read_u32().is_err());

        assert_eq!(decoder.offset(), 20);
        assert_eq!(decoder.remaining(), 3);
        assert!(!decoder.is_empty());
    }

    #[test]
    fn truncation_after_partial_consumption_reports_input_end() {
        let bytes = [1, 2, 3];
        let mut decoder = Decoder::new(&bytes, 40);

        assert_eq!(decoder.read_u8().unwrap(), 1);
        assert_eq!(decoder.read_u32(), Err(Truncated { offset: 43 }));
        assert_eq!(decoder.offset(), 41);
        assert_eq!(decoder.remaining(), 2);
    }
}
