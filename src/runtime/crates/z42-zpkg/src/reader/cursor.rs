use super::*;

// ── Low-level reader helpers ──────────────────────────────────────────────────

pub(super) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self { Cursor { data, pos: 0 } }

    pub(super) fn remaining(&self) -> usize { self.data.len() - self.pos }

    pub(super) fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() { bail!("unexpected end of data (u8)") }
        let v = self.data[self.pos]; self.pos += 1; Ok(v)
    }
    pub(super) fn read_u16(&mut self) -> Result<u16> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos+1]]);
        self.pos += 2; Ok(v)
    }
    pub(super) fn read_u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().unwrap());
        self.pos += 4; Ok(v)
    }
    pub(super) fn read_i32(&mut self) -> Result<i32> {
        self.need(4)?;
        let v = i32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().unwrap());
        self.pos += 4; Ok(v)
    }
    pub(super) fn read_i64(&mut self) -> Result<i64> {
        self.need(8)?;
        let v = i64::from_le_bytes(self.data[self.pos..self.pos+8].try_into().unwrap());
        self.pos += 8; Ok(v)
    }
    pub(super) fn read_f64(&mut self) -> Result<f64> {
        self.need(8)?;
        let v = f64::from_le_bytes(self.data[self.pos..self.pos+8].try_into().unwrap());
        self.pos += 8; Ok(v)
    }
    pub(super) fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.data[self.pos..self.pos+n]; self.pos += n; Ok(s)
    }
    /// Unsigned LEB128 varint (STRS segment-dict). Max 5 bytes for u32.
    pub(super) fn read_varint(&mut self) -> Result<u32> {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;
        for _ in 0..5 {
            let b = self.read_u8()?;
            result |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 { return Ok(result); }
            shift += 7;
        }
        bail!("varint too long (>5 bytes)")
    }
    pub(super) fn read_utf8_u16len(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        let b = self.read_bytes(len)?;
        Ok(std::str::from_utf8(b)?.to_owned())
    }
    pub(super) fn need(&self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() { bail!("unexpected end of data") }
        Ok(())
    }
    pub(super) fn pool_str<'p>(&self, pool: &'p [String], idx: u32) -> Result<&'p str> {
        pool.get(idx as usize)
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("string pool index {} out of range (pool size {})", idx, pool.len()))
    }
}
